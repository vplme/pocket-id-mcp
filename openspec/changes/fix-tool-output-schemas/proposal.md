# Proposal: fix-tool-output-schemas

## Why

Claude Code rejects this server's entire tool list ("tools fetch failed") because 14 tools declare an `outputSchema` whose root is not a JSON object. The MCP schema (`schema.ts`, all revisions) types `outputSchema` as `{ type: "object", ... }` and `structuredContent` as a string-keyed object — an array or primitive root is not representable. rmcp derives output schemas verbatim from the Rust return type, so tools returning `Json<Vec<T>>` emit an illegal `{"type": "array"}` root and tools returning `Json<serde_json::Value>` emit a type-less "anything" schema. The server is currently unusable from Claude Code over any transport.

## What Changes

- Wrap every non-object structured tool result in a `{"result": <value>}` envelope, following the official Python SDK (FastMCP) convention for non-object returns. The derived `outputSchema` root becomes `{"type": "object", "properties": {"result": ...}, "required": ["result"]}`.
- Affected tools (8 array-rooted, 6 type-less):
  `list_user_groups_of_user`, `list_user_passkeys`, `get_all_application_configuration`, `get_public_application_configuration`, `update_application_configuration`, `update_group_custom_claims`, `update_user_custom_claims`, `create_one_time_access_token`, `get_current_version`, `get_latest_version`, `get_custom_claim_suggestions`, `introspect_token`, `list_audit_log_client_names`, `list_audit_log_users`.
- Add a regression test asserting every registered tool's `outputSchema` root is `"type": "object"`, so a future tool with a bare array/primitive return can never ship.
- **BREAKING** (nominally): the `structuredContent` shape of the 14 tools changes from a bare array/value to `{"result": ...}`. In practice nothing breaks — these tools were never consumable from Claude Code, and no released version exists.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `server-runtime`: The "Structured tool output schemas" requirement gains the MCP conformance constraint — every declared output schema SHALL have an object root, with non-object results wrapped in a `result` envelope.

## Impact

- `src/tools/mod.rs` (or `src/dto.rs`): a small generic `Wrapped<T>`-style envelope type deriving `Serialize`/`JsonSchema`.
- `src/tools/identity.rs`, `src/tools/admin.rs`, `src/tools/oidc.rs`: the 14 affected tool signatures change from `Json<Vec<T>>`/`Json<serde_json::Value>` to the enveloped form; handler bodies wrap their value.
- `tests/tiers.rs` (or a new test): outputSchema-root regression test.
- No dependency changes, no config changes, no transport changes.
