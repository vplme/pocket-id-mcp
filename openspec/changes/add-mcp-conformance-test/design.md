# Design: add-mcp-conformance-test

## Context

Claude Code validates `tools/list` responses with the official TypeScript SDK's zod types, which mirror `schema.ts` in the modelcontextprotocol spec repository. The same repository publishes `schema.json` per protocol revision — the machine-readable ground truth. This repo already has a pattern for vendored external contracts: `spec/` holds the Pocket ID OpenAPI document with a coverage test and drift CI. rmcp 3.1.2's newest supported revision is 2026-07-28.

## Goals / Non-Goals

**Goals:**
- Definition-level protocol conformance verified in `cargo test`, with violations reported by JSON path (same shape as Claude Code's zod errors).
- Catch the whole class: root types, property-value types, and any constraint we have not yet met a strict client for.

**Non-Goals:**
- Layer 2 (an e2e smoke test driving the built binary with the official TS SDK client) — deferred unless layer 1 proves insufficient.
- Validating runtime *results* (`structuredContent` against each tool's own outputSchema) — rmcp produces both from the same type, so definition-level checks cover the declared contract.
- Schema drift automation for the MCP schema (the vendored revision only changes when the rmcp dependency's protocol support changes, which is a deliberate, reviewed bump — unlike the Pocket ID API, which drifts on its own).

## Decisions

### 1. Vendor `schema.json` for two revisions, don't fetch in CI

`spec/mcp-schema-2025-06-18.json` and `spec/mcp-schema-2026-07-28.json`, committed. Tests must run offline and reproducibly; the schema for a published revision is immutable, so vendoring has no staleness cost.

Why two: rmcp negotiates the protocol revision down to what the client speaks (it supports 2025-03-26 through 2026-07-28), so advertised definitions must satisfy the strictest negotiable revision, not just the newest. Inspecting the published schemas showed the constraint that broke Claude Code — `outputSchema: {type: "object"}` with object-valued `properties` — exists in 2025-06-18 but was **dropped** in 2026-07-28 ("any valid JSON Schema 2020-12"). Validating only the newest revision would have caught neither observed failure. 2025-06-18 is the strict floor (2025-03-26 predates `outputSchema` entirely); 2026-07-28 is kept for constraints the new revision adds elsewhere. The coupling rule, documented in the test header: newest vendored revision = newest `ProtocolVersion` in the pinned rmcp.

### 2. Validate each `Tool` definition via a root `$ref`

Each schema document keeps every type under `definitions` (draft-07, 2025-06-18) or `$defs` (draft 2020-12, 2026-07-28) with no root type. The test loads the document, sets a root `"$ref"` to its `Tool` definition, compiles it with the `jsonschema` crate (which auto-detects the draft), and validates every serialized rmcp tool definition, prefixing violations with the tool name. Violations are collected via `iter_errors` and reported as `[tool] revision path: message`.

Alternative considered: validating the whole `ListToolsResult` — rejected after trying it: 2026-07-28 added required runtime envelope fields (`resultType`, `cacheScope`, `ttlMs`) that the serving SDK stamps onto responses at request time; a test-constructed `{"tools": [...]}` fails on fields that are rmcp's runtime responsibility, not part of the definitions under test.

### 3. All tiers enabled

The test builds the server with `POCKET_ID_MCP_ALLOW_DANGEROUS=true` so dangerous-tier tools are validated too — read-only/default modes advertise subsets of the same definitions.

### 4. Keep the hand-written assertions

The targeted assertions in `tests/tiers.rs` (object roots, object property values) stay: they fail with sharper messages naming the offending tool and rule, while the schema test is the exhaustive net. Redundancy here is cheap and intentional.

## Risks / Trade-offs

- [Claude Code's zod could be stricter than the published schema in some corner] → Accepted for layer 1; that gap is exactly what layer 2 (TS SDK client smoke test) would close. No known instance today — both observed failures are encoded in the published schema.
- [rmcp bump to a newer protocol revision silently outruns the vendored schema] → The coupling is documented in the test header; the revision is in the filename so a mismatch is visible in review. Full automation is out of scope.
- [`jsonschema` crate adds a dev-dependency tree] → Dev-only; no runtime impact.

## Migration Plan

None — additive test infrastructure. Lands on top of the `fix-anyjson-property-schemas` fixes (PR #14); on any earlier tree the test fails by design.

## Open Questions

None.
