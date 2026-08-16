# Design: fix-tool-output-schemas

## Context

rmcp's `Json<T>` tool return derives `outputSchema` from `T` via schemars and sends the serialized value as `structuredContent`. The MCP protocol types both as object-rooted (`outputSchema: { type: "object", ... }`, `structuredContent: { [key: string]: unknown }`). Claude Code validates tool definitions at `tools/list` time with zod and rejects the whole list on the first violation; MCP Inspector does not validate, which masked the bug. 14 of 84 tools violate the constraint — diagnosed live against the router: 8 `Json<Vec<T>>` returns produce `"type": "array"` roots, and 6 `Json<serde_json::Value>` returns produce type-less schemas.

## Goals / Non-Goals

**Goals:**
- Every registered tool's `outputSchema` root is `"type": "object"`; Claude Code loads the full tool list.
- Keep structured output (don't just drop `outputSchema` from offending tools).
- Regression-proof: adding a new bare-array tool must fail CI.

**Non-Goals:**
- Renaming per-tool response fields or hand-crafting domain DTO wrappers (`{"groups": [...]}`, `{"passkeys": [...]}`) — more churn for the same conformance.
- Changing tools whose schemas are already object-rooted.
- Patching rmcp upstream (worth an issue, not a blocker).

## Decisions

### 1. Generic `result` envelope, FastMCP convention

A single generic wrapper in `src/dto.rs`:

```rust
/// MCP requires object-rooted structured content; non-object results are
/// wrapped as {"result": ...} (the convention the official Python SDK uses).
#[derive(Serialize, JsonSchema)]
pub struct Enveloped<T> {
    pub result: T,
}
```

Affected handlers change `Ok(Json(value))` → `Ok(Json(Enveloped { result: value }))` and their signature to `Json<Enveloped<...>>`. The derived schema becomes `{"type":"object","properties":{"result":...},"required":["result"]}`.

**Property schemas must be objects too.** The MCP `Tool` type constrains not only the root (`type: "object"`) but each entry under `properties` to be an object (`properties?: {[key: string]: object}`). schemars emits the *boolean* schema `true` for `serde_json::Value`, which strict clients reject at `outputSchema.properties.<key>` (observed live from Claude Code). Freeform values therefore use an `AnyJson` newtype (`#[serde(transparent)]` over `serde_json::Value`) with a manual `JsonSchema` impl emitting the unconstrained *object* schema `{}` — semantically identical to `true`, structurally an object. This applies both to `Enveloped<AnyJson>` tool returns and to freeform fields inside response DTOs that form top-level schema properties (`OidcClientPreview.access_token`/`id_token`/`user_info`).

Alternative considered: omit `outputSchema` on the 14 tools (spec-legal — it's optional) — rejected: loses validation and type information, and rmcp's `Json<T>` couples schema derivation to the return type anyway, so omission would mean switching those tools to unstructured text returns.

Alternative considered: FastMCP-style automatic wrapping inside a custom rmcp adapter — over-engineered for a fixed set of 14 call sites in one repo.

### 2. Regression test at the definition level

Extend the existing schema test in `tests/tiers.rs` (which already iterates registered tool definitions) with: for every tool, if `output_schema` is present, its root `"type"` must equal `"object"` **and** every value under its `properties` must be a JSON object (not a boolean schema). Runs with all tiers enabled so dangerous-tier tools are covered too. This mirrors exactly the shape the MCP `Tool` type (and Claude Code's validation) enforces.

### 3. Text content follows the envelope

rmcp serializes the same wrapped value into the backwards-compatible text block, so the text representation also gains the `{"result": ...}` wrapper. Accepted: consistency between text and structured content is exactly what the spec's backwards-compat note asks for.

## Risks / Trade-offs

- [Envelope is mild noise for LLM consumers reading `result.result`-style paths] → One predictable key beats 14 bespoke shapes; FastMCP has normalized this convention across the ecosystem.
- [Future contributor adds a `Json<Vec<T>>` tool] → The regression test fails CI with a message naming the tool.
- [Other MCP clients already integrated against the bare-array shape] → None exist; the server is pre-release and Claude Code (the primary target) could never load it.

## Migration Plan

None: no released consumers. Merge independently of (before or after) the `add-http-auth-modes` branch; the changes touch disjoint files.

## Open Questions

None.
