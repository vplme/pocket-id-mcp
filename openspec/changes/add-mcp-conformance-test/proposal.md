# Proposal: add-mcp-conformance-test

## Why

Two rounds of Claude Code rejecting the entire tool list ("tools fetch failed") were caught only by pointing a strict client at a live server: hand-written tests asserted what we *thought* the protocol required (outputSchema present; then object-rooted; then object-valued properties) and each round revealed another constraint. The MCP spec publishes its canonical type definitions as machine-readable JSON Schema per protocol revision, and Claude Code's validation (the official TypeScript SDK's zod types) mirrors exactly those definitions — so validating our advertised definitions against the published schema catches this entire class of bug at `cargo test` time.

## What Changes

- Vendor the MCP JSON Schema for **two** protocol revisions, alongside the already-vendored Pocket ID OpenAPI spec: **2025-06-18** (`spec/mcp-schema-2025-06-18.json`) and **2026-07-28** (`spec/mcp-schema-2026-07-28.json`, the newest revision the pinned rmcp 3.1.2 supports). Two because clients negotiate the revision downward and the definitions must satisfy the strictest negotiable one: 2025-06-18 constrains `outputSchema` to `{type: "object", properties: {[k]: object}}` (what Claude Code's SDK-derived validation enforces — both observed failures), while 2026-07-28 loosened `outputSchema` to "any valid JSON Schema 2020-12" and would have caught neither.
- Add a conformance test that builds the server with all tiers enabled, serializes the advertised tool list as a `ListToolsResult`, and validates it against each vendored schema's `ListToolsResult` definition, reporting each violation with its JSON path.
- Add the `jsonschema` crate as a dev-dependency.
- Document the vendored revision and its coupling to the pinned rmcp version (bumping rmcp to a newer protocol revision means re-vendoring the matching schema).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `server-runtime`: adds a requirement that advertised tool (and prompt) definitions validate against the vendored MCP protocol schema for the pinned revision.

## Impact

- `spec/mcp-schema-2026-07-28.json`: new vendored file (from the modelcontextprotocol spec repository).
- `tests/mcp_conformance.rs`: new test.
- `Cargo.toml`: `jsonschema` dev-dependency.
- Depends on the `fix-anyjson-property-schemas` fixes (PR #14) — the test fails on any pre-fix tree, which is the point.
