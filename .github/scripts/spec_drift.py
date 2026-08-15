#!/usr/bin/env python3
"""Diff the operation sets of two swagger specs.

Usage: spec_drift.py <vendored.yaml> <upstream.yaml>

Prints a markdown drift report to stdout; prints nothing when the operation
sets (path + method + parameter names) are identical.
"""

import sys

import yaml

METHODS = {"get", "post", "put", "delete", "patch"}


def operations(path):
    with open(path) as f:
        spec = yaml.safe_load(f)
    ops = {}
    for p, item in (spec.get("paths") or {}).items():
        for method, op in (item or {}).items():
            if method in METHODS:
                params = sorted(
                    f"{param.get('in', '?')}:{param.get('name', '?')}"
                    for param in (op or {}).get("parameters", [])
                )
                ops[(method.upper(), p)] = params
    return ops


def main():
    vendored = operations(sys.argv[1])
    upstream = operations(sys.argv[2])

    added = sorted(set(upstream) - set(vendored))
    removed = sorted(set(vendored) - set(upstream))
    changed = sorted(
        op for op in set(vendored) & set(upstream) if vendored[op] != upstream[op]
    )

    if not (added or removed or changed):
        return

    print("Upstream `swagger.yaml` at pocket-id.org no longer matches the vendored copy in `spec/swagger.yaml`.")
    print()
    if added:
        print("### Added upstream (not in vendored spec)")
        for m, p in added:
            print(f"- `{m} {p}`")
        print()
    if removed:
        print("### Removed upstream (still in vendored spec)")
        for m, p in removed:
            print(f"- `{m} {p}`")
        print()
    if changed:
        print("### Changed parameters")
        for m, p in changed:
            print(f"- `{m} {p}`")
            print(f"  - vendored: {', '.join(vendored[(m, p)]) or '(none)'}")
            print(f"  - upstream: {', '.join(upstream[(m, p)]) or '(none)'}")
        print()
    print(
        "To reconcile: update `spec/swagger.yaml`, adjust the tool catalog or "
        "`spec/exclusions.toml`, and let the coverage test enforce completeness."
    )


if __name__ == "__main__":
    main()
