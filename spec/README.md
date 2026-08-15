# Vendored Pocket ID API spec

- `swagger.yaml` — downloaded from <https://pocket-id.org/swagger.yaml> on 2026-08-15.
- Corresponds to Pocket ID **v2.13.0** (latest release at vendoring time, published 2026-08-07).
- 103 operations across 74 paths (Swagger 2.0).

This file is the source of truth for API coverage accounting: the coverage test
(`tests/coverage.rs`) asserts every operation here is either mapped to an MCP tool
or listed in `exclusions.toml` with a reason. Update it deliberately — a spec bump
must be reconciled with the tool surface before CI passes.
