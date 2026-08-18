# Proposal: reject-unknown-tool-params

## Why

A tool call carrying a misspelled or invented parameter is silently accepted today: serde drops unknown keys at the MCP layer, and Pocket ID drops unknown `filters[...]` keys server-side with a 200. Stacked, a wrong call returns real-looking data answering a different question — observed live when `list_all_audit_logs` called with REST-style `filters`/`sort` objects returned 354 unfiltered rows across all users instead of the 241 the caller asked for. For an audit-log endpoint, a silently over-broad result is the expensive failure mode, and nothing in the response indicates the parameters were ignored.

## What Changes

- Add `#[serde(deny_unknown_fields)]` to every struct used as `Parameters<T>` in a tool signature (~45 structs across `src/tools/admin.rs`, `src/tools/identity.rs`, `src/tools/oidc.rs`, plus `ListParams` and the dual-role input structs in `src/dto.rs`/`src/client.rs`). A call with an unknown top-level key now fails as a JSON-RPC `-32602` invalid-params error — the MCP-spec-sanctioned channel for invalid arguments, whose message text (serde's `unknown field ...`) rmcp forwards to the client — instead of running with the key discarded.
- Advertise the strictness: inject `additionalProperties: false` at the top level of every tool input schema, in the existing schema post-processing loop in `PocketIdServer::new` (the same place `collapse_nullable_types` runs), so schema-aware clients can reject or avoid bad calls before the round-trip.
- Replace the two prose-documented closed value sets with unit enums, moving validation into the schema and the serde layer: `sort_direction` (`asc`/`desc`, shared via `ListParams`) and `list_all_audit_logs`' `location` (`internal`/`external`, currently silently ignored upstream for any other value). Invalid values now fail with serde's built-in did-you-mean (`unknown variant 'ascending', expected 'asc' or 'desc'`). `event` stays a free string (open, server-evolving set); Go-duration `ttl` fields stay strings.
- Extend the input-schema transform to normalize what schemars emits for `Option<UnitEnum>`: collapse `anyOf: [X, {"type": "null"}]` to `X` and inline the enum subschema, so optional enum params advertise a plain `{"type": "string", "enum": [...]}` — preserving the form-rendering guarantee established by `fix-nullable-param-schemas`.
- **BREAKING** (nominally): calls that previously succeeded while silently dropping unknown keys or invalid `sort_direction`/`location` values now return errors. This is the point of the change; no released version exists.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `server-runtime`: gains a requirement that tool input handling is closed-world — unknown top-level parameters are rejected as invalid-params protocol errors, advertised input schemas declare `additionalProperties: false`, and closed parameter value sets are advertised as schema enums rather than documented in prose.

## Impact

- `src/tools/admin.rs`, `src/tools/identity.rs`, `src/tools/oidc.rs`: `deny_unknown_fields` on all param structs; `AuditLogFilterParams.location` becomes an enum.
- `src/dto.rs`: `deny_unknown_fields` on `ListParams` and the input structs used directly as tool params; `sort_direction` becomes an enum; new `SortDirection` / `AuditLogLocation` unit enums.
- `src/client.rs`: `deny_unknown_fields` on `FileSource` (used both directly and flattened).
- `src/server.rs`: the input-schema post-processing loop gains `additionalProperties: false` injection and the `anyOf`-with-null collapse / enum inlining.
- Tests: regression tests over `registered_tools()` (every input schema top level declares `additionalProperties: false`; no `anyOf` in input schemas); serde round-trip tests for unknown-key rejection including the dual-role flatten structs; extension of the `tests/mcp_conformance.rs` negative control.
- Coordination: depends on `fix-nullable-param-schemas` (its transform is already merged on main; this change extends the same loop — archive it first). `fix-tool-output-schemas` touches output schemas only; the delta specs add separate requirements and do not overlap.
- No dependency, config, or transport changes.
