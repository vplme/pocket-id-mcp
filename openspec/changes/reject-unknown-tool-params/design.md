# Design: reject-unknown-tool-params

## Context

Unknown tool-call parameters are dropped twice today: serde discards unknown keys when deserializing `Parameters<T>` (no struct uses `deny_unknown_fields`), and Pocket ID ignores unknown `filters[...]` query keys with a 200. A model that mimics the REST wire format (`filters: {userId}`, `sort: {column}`) instead of the tool's flat params gets a plausible-looking but wrong answer — observed live on `list_all_audit_logs` (354 unfiltered rows vs the 241 correct ones).

Facts this design rests on (all verified empirically, not from memory):

- rmcp 3.1.2's `Parameters<P>` is `#[serde(transparent)]`, so `deny_unknown_fields` on our structs propagates. A serde failure becomes a JSON-RPC `-32602` invalid-params protocol error with serde's message text (built in rmcp's `tool.rs`), which is the channel the MCP spec itself designates for invalid arguments.
- `deny_unknown_fields` works alongside `#[serde(flatten)]` (scratch-crate verified): unknown keys are rejected, inner fields still parse. The only degradation is the error message losing its expected-field list (`unknown field 'filters'` instead of `... expected one of 'page', ...`).
- Dual-role structs are safe (scratch-crate verified): a struct carrying `deny_unknown_fields` can be used both directly as `Parameters<T>` and as a flattened inner field of another deny struct. This matters because `ListParams`, `UserInput`, `UserGroupInput`, `OidcClientInput`, `ScimServiceProviderInput`, and `FileSource` all play both roles.
- schemars 1.x emits `Option<UnitEnum>` as `anyOf: [{"$ref": "#/$defs/E"}, {"type": "null"}]` — not a `["X","null"]` type array — so the existing `collapse_nullable_types` transform does not normalize it.

Coordination: `fix-nullable-param-schemas` (in progress, 3/4, code merged on main) owns the input-schema post-processing loop in `PocketIdServer::new` that this change extends; archive it before implementing this one. `fix-tool-output-schemas` touches output schemas only — no overlap.

## Goals / Non-Goals

**Goals:**

- A tool call with an unknown top-level parameter fails loudly instead of running with the key silently dropped.
- The advertised input schemas tell the truth about this strictness (`additionalProperties: false`), so schema-aware clients can catch the mistake before the round-trip.
- Closed parameter value sets (`sort_direction`, audit-log `location`) are machine-readable schema enums, not doc-comment prose, closing the "valid per schema but silently ignored upstream" gap for those fields.
- Optional enum params render as typed inputs in form-rendering clients (MCP Inspector), preserving the guarantee `fix-nullable-param-schemas` established.

**Non-Goals:**

- No custom pre-deserialization validation layer or did-you-mean errors in tool results. The `-32602` channel with serde's message is a sufficient correction signal for model callers (which hold the tool schema in context); a hand-written `call_tool` override would add a second source of validation truth for marginal wording gains. Revisit only if a client is shown to swallow protocol errors.
- No strictness inside nested objects (`claims` array items, the deliberately open `config` value). Nested input structs have mostly required fields, so misspellings there already surface as missing-field errors.
- No enum for the audit-log `event` filter (open, server-evolving set) and no structural typing of Go-duration `ttl` strings (a schema `pattern` could be added later; not part of this change).
- No change to output schemas or to the upstream client layer.

## Decisions

**1. Reject via `deny_unknown_fields` on every `Parameters<T>` struct — strict everywhere, not just list/filter tools.**
Uniformity costs nothing extra, has no legitimate unknown-key use case to preserve (MCP clients do not inject vendor keys into `arguments`; `_meta` lives outside it), and makes the regression test a single uniform assertion. Selective strictness invites drift.
*Alternative considered*: strict only where the failure is expensive (audit/list tools). Rejected — deciding per-tool costs more than applying the attribute everywhere, and "cheap" silent failures are still failures.

**2. Let the rejection surface as the `-32602` protocol error rather than a `CallToolResult { isError: true }`.**
The MCP spec explicitly lists invalid arguments as a JSON-RPC `-32602` protocol error, rmcp forwards serde's message text in it, and Claude Code relays MCP protocol errors to the model as visible tool-error text. The load-bearing property is any rejection instead of silence; the model self-corrects from `unknown field 'filters'` plus the schema it already has.
*Alternative considered*: a pre-deserialization check against `inputSchema.properties` rejecting into a tool result with did-you-mean suggestions. Rejected per Non-Goals — cost (custom `call_tool` override, duplicate validation truth) outweighs the nicer wording.

**3. Advertise `additionalProperties: false` at the top level of every input schema, injected in the existing transform loop.**
JSON Schema is open-world by default, so without this the schema would advertise looser behavior than the server enforces. Top-level-only exactly matches where `deny_unknown_fields` applies (serde flatten merges everything to the top level, and schemars merges flattened properties the same way). Injection in `PocketIdServer::new` beats per-struct `#[schemars(deny_unknown_fields)]`-style annotations because it is one line, cannot be forgotten on a new tool, and sits next to the transform that already owns schema normalization.

**4. Unit enums for the two genuinely closed sets, with serde giving did-you-mean for free.**
`SortDirection { Asc, Desc }` (in `dto.rs`, used by `ListParams.sort_direction`) and `AuditLogLocation { Internal, External }` (for `AuditLogFilterParams.location`), both `#[serde(rename_all = "snake_case")]` (matching the existing `ImageType` precedent) and `#[schemars(inline)]`. Serde's variant error (`unknown variant 'ascending', expected 'asc' or 'desc'`) is a better correction signal than anything hand-rolled. Query serialization uses a `&'static str` accessor per enum, keeping `to_query` untouched in shape.

**5. Extend the schema transform to collapse `anyOf`-with-null and rely on `#[schemars(inline)]` for enum inlining.**
Without this, the new `Option<enum>` params advertise `anyOf: [{"$ref"}, {"type": "null"}]`, regressing MCP Inspector's typed-input rendering. The transform gains: where a schema object is exactly `anyOf: [X, {"type": "null"}]`, replace it with `X` (merging any sibling keys like `description`). `#[schemars(inline)]` keeps the enum out of `$defs` so no `$ref` resolution is needed. The existing `collapse_nullable_types` stays as-is; the new collapse is a sibling function applied in the same loop.

## Risks / Trade-offs

- [Flatten structs lose the expected-field list in the error (`unknown field 'filters'` only)] → Acceptable: the caller is a model holding the full input schema in context; the tool name + unknown-field name is enough to re-derive the correct call. Non-flatten structs (the majority) keep the full list.
- [A future client that swallows `-32602` errors would turn rejection back into silence] → The advertised `additionalProperties: false` gives such clients a second chance to catch it pre-call; if a concrete client is identified, the tool-result channel can be revisited as an additive change.
- [`deny_unknown_fields` + `flatten` is officially unsupported by serde docs; behavior verified empirically on current serde] → Pin the guarantee with round-trip tests for every dual-role struct (valid-flatten parse, unknown-key rejection in both roles), so a serde behavior change fails CI rather than silently reopening the hole.
- [`additionalProperties: false` on schemas whose struct gains a field later is self-maintaining, but a hand-added tool bypassing the transform loop would advertise open-world while enforcing closed-world] → The regression test asserts the invariant over `registered_tools()`, which covers every registered tool by construction.
- [Stricter behavior could break an existing caller that (harmlessly) sends extra keys] → Nominally breaking, accepted: no released version exists, and any such caller is currently getting silently wrong behavior.

## Migration Plan

1. Land after `fix-nullable-param-schemas` is archived (same transform loop).
2. Attribute sweep + enums + transform extension + tests in one PR; `cargo fmt` / `clippy` / `test` gate as usual.
3. Rollback = revert; no data, config, or protocol migration.

## Open Questions

None blocking. Optional follow-ups deliberately left out: schema `pattern` for Go-duration `ttl` fields; enum-ifying the audit-log `event` filter if upstream ever publishes a closed list.
