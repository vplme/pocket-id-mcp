# Tasks: fix-nullable-param-schemas

## 1. Schema transform

- [x] 1.1 Add `collapse_nullable_types` in `src/server.rs` and apply it to every registered tool's input schema in `PocketIdServer::new`

## 2. Regression test

- [x] 2.1 `tests/tiers.rs`: assert no registered tool's input schema contains a type array; spot-check `get_oidc_client_logo.light` is `"type": "boolean"`

## 3. Verification

- [x] 3.1 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` all pass
- [ ] 3.2 Manual check: MCP Inspector renders `light` as a true/false toggle instead of a text field
