# Proposal: fix-nullable-param-schemas

## Why

schemars 1.x maps every `Option<T>` parameter to a nullable type array — `"type": ["boolean", "null"]` for `Option<bool>` — in the derived input schema. Form-rendering MCP clients such as MCP Inspector only produce typed inputs (a boolean toggle, a number field) for a plain `"type"` string; a type array falls back to a raw JSON text field, where each edit re-escapes the previous value and the submitted argument is a mangled string instead of `true`. The `null` branch buys nothing for MCP callers: optionality is already expressed by the property's absence from `required`, and callers omit optional params rather than sending explicit null.

## What Changes

- At router construction, post-process every advertised tool input schema: recursively collapse `"type": ["X", "null"]` to `"type": "X"` (dropping `"null"` from longer arrays too).
- Input schemas only. Output schemas are left untouched: responses may legitimately serialize optional fields as `null`, and strict clients validate structured content against the declared output schema.
- Deserialization is unchanged — the handlers still accept omitted params (and explicit null) via `Option<T>`; only the advertised schema narrows.
- Add a regression test asserting no registered tool's input schema contains a type array, with a spot-check that `get_oidc_client_logo.light` is advertised as a plain `"boolean"`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `server-runtime`: gains a requirement that advertised input schemas use plain types for optional parameters, expressing optionality via `required` alone.

## Impact

- `src/server.rs`: a `collapse_nullable_types` transform applied to each `ToolRoute`'s input schema in `PocketIdServer::new`.
- `tests/tiers.rs`: `input_schemas_use_plain_types_for_optional_params` regression test.
- No dependency, config, or transport changes; no tool behavior changes.
