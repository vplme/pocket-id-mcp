# Tasks: reject-unknown-tool-params

## 1. Prerequisite

- [ ] 1.1 Confirm `fix-nullable-param-schemas` is archived (its `collapse_nullable_types` loop in `src/server.rs` is the extension point; do not implement this change while that one is still in flight)

## 2. Closed value sets as enums

- [ ] 2.1 Add `SortDirection { Asc, Desc }` unit enum in `src/dto.rs` (`#[serde(rename_all = "snake_case")]`, `#[schemars(inline)]`, `as_str()` accessor); switch `ListParams.sort_direction` to `Option<SortDirection>` and update `to_query`
- [ ] 2.2 Add `AuditLogLocation { Internal, External }` enum in `src/tools/admin.rs` (same attributes); switch `AuditLogFilterParams.location` to `Option<AuditLogLocation>`, update `to_query`, and trim the now-redundant doc-comment prose on both fields

## 3. Reject unknown fields

- [ ] 3.1 Add `#[serde(deny_unknown_fields)]` to every struct used as `Parameters<T>`: all param structs in `src/tools/admin.rs`, `src/tools/identity.rs`, `src/tools/oidc.rs`, plus `ListParams`, `UserInput`, `UserGroupInput`, `OidcClientInput`, `ScimServiceProviderInput` in `src/dto.rs` and `FileSource` in `src/client.rs` (dual-role: direct and flattened — verified safe). Do not add it to inner-only DTO types that never appear in `Parameters<T>`

## 4. Schema transform extension

- [ ] 4.1 In `src/server.rs`, add a transform collapsing `anyOf: [X, {"type": "null"}]` to `X` (preserving sibling keys such as `description`) and apply it in the input-schema loop alongside `collapse_nullable_types`
- [ ] 4.2 In the same loop, insert `"additionalProperties": false` at the top level of every tool input schema

## 5. Tests

- [ ] 5.1 Regression test over `registered_tools()`: every input schema top level declares `additionalProperties: false`; no input schema contains `anyOf` or `$ref`; spot-check `sort_direction` on a list tool is inline `{"type": "string", "enum": ["asc", "desc"]}` and `location` on `list_all_audit_logs` is `["internal", "external"]`
- [ ] 5.2 Serde round-trip tests: unknown top-level key rejected for a plain struct, a flattened struct (`AuditLogFilterParams` with a `filters` key), and each dual-role struct in both roles (direct `ListParams`/`FileSource` and flattened via `SearchListParams`/`UpdateImageParams`); valid flattened input still parses; invalid enum value error names the valid variants
- [ ] 5.3 Extend the `tests/mcp_conformance.rs` negative control to cover the strictness invariant (a schema without `additionalProperties: false` fails the check)

## 6. Verification

- [ ] 6.1 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` all pass
- [ ] 6.2 Manual check with MCP Inspector: unknown param call returns a visible invalid-params error; `sort_direction` renders as a select/enum input
