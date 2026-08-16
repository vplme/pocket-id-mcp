# Tasks: add-mcp-conformance-test

## 1. Vendored schema

- [x] 1.1 Vendor the MCP JSON Schema for revisions 2025-06-18 (strict floor: object-rooted `outputSchema` with object-valued properties) and 2026-07-28 (newest rmcp-supported) as `spec/mcp-schema-<rev>.json` (from the modelcontextprotocol spec repository)

## 2. Conformance test

- [x] 2.1 Add `jsonschema` as a dev-dependency
- [x] 2.2 Add `tests/mcp_conformance.rs`: all-tiers server, serialize tool list as `ListToolsResult`, validate against `#/definitions/ListToolsResult` via root `$ref`, report violations by instance path; header comment documents the rmcp↔schema revision coupling

## 3. Verification

- [x] 3.1 Confirm the test fails on a pre-fix tree (revert check or equivalent) and passes on the current tree
- [x] 3.2 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` all pass
