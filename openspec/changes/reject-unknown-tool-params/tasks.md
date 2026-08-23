# Tasks: reject-unknown-tool-params

## 1. Prerequisite

- [x] 1.1 Confirm `fix-nullable-param-schemas` poses no code conflict. (Deviation from original wording: it is not yet archived, but its `collapse_nullable_types` code is fully merged on main with only the manual Inspector check outstanding, and the user explicitly requested apply — no in-flight edits touch the transform loop. Archive ordering now only matters for spec syncing, where both deltas are separate ADDED requirements.)

## 2. Closed value sets as enums

- [x] 2.1 Add `SortDirection { Asc, Desc }` unit enum in `src/dto.rs` (`#[serde(rename_all = "snake_case")]`, `#[schemars(inline)]`, `as_str()` accessor); switch `ListParams.sort_direction` to `Option<SortDirection>` and update `to_query`
- [x] 2.2 Add `AuditLogLocation { Internal, External }` enum in `src/tools/admin.rs` (same attributes); switch `AuditLogFilterParams.location` to `Option<AuditLogLocation>`, update `to_query`, and trim the now-redundant doc-comment prose on both fields. (`ImageType` deliberately keeps its `$defs`/`$ref` shape: its variant doc comments derive as a documented `oneOf`, worth more than inlining.)

## 3. Reject unknown fields

- [x] 3.1 Add `#[serde(deny_unknown_fields)]` to every struct used as `Parameters<T>` in a tool signature: 47 structs across `src/tools/admin.rs` (10), `src/tools/identity.rs` (16), `src/tools/oidc.rs` (15), `src/dto.rs` (`ListParams`, `UserInput`, `UserGroupInput`, `OidcClientInput`, `ScimServiceProviderInput`), and `src/client.rs` (`FileSource`). Inner-only DTO types (e.g. `OidcClientUpdateInput`, `ScimServiceProviderUpdateInput`) and prompt-argument structs left unchanged

## 4. Schema transform extension

- [x] 4.1 In `src/server.rs`: `collapse_nullable_types` now also drops `null` from a sibling `enum` list when it removes the null branch of a type array (the shape `#[schemars(inline)]` actually produces for `Option<enum>`); a `collapse_nullable_anyof` sibling function additionally collapses `anyOf: [X, {"type": "null"}]` to `X` as defense, applied in the same loop
- [x] 4.2 In the same loop, insert `"additionalProperties": false` at the top level of every tool input schema

## 5. Tests

- [x] 5.1 `tests/strict_params.rs`: every registered tool's input schema top level declares `additionalProperties: false`; no input schema contains `anyOf`; spot-checks that `list_users.sort_direction` is inline `{"type": "string", "enum": ["asc", "desc"]}` and `list_all_audit_logs.location` is `["internal", "external"]`. (`$ref` is permitted for nested object DTOs — pre-existing shape, out of scope.)
- [x] 5.2 Serde round-trip tests in the same file: unknown top-level key rejected for a plain struct and for `AuditLogFilterParams` fed the exact REST-style `filters`/`sort` mimicry observed live; dual-role structs (`ListParams`, `FileSource`) stay strict and functional in both direct and flattened roles; invalid enum values produce errors naming the valid variants
- [x] 5.3 Negative control for the strictness invariant lives in `tests/strict_params.rs` (an open-world schema and an un-collapsed `anyOf` schema are both flagged). (Deviation from original wording: `tests/mcp_conformance.rs` validates against the MCP meta-schema, which cannot express this project-level invariant, so the control belongs beside the checker it exercises.)

## 6. Verification

- [x] 6.1 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` all pass (66 tests, including 6 new strict-params tests)
- [ ] 6.2 Manual check with MCP Inspector: unknown param call returns a visible invalid-params error; `sort_direction` renders as a select/enum input
