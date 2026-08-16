# server-runtime (delta)

## ADDED Requirements

### Requirement: Plain types for optional input parameters
Advertised tool input schemas SHALL use plain `"type"` strings for optional parameters, expressing optionality solely through absence from `required`. Nullable type arrays produced by schema derivation (e.g. `"type": ["boolean", "null"]` for `Option<bool>`) SHALL be collapsed to the plain type before the tool list is served, so form-rendering clients (MCP Inspector) present typed inputs and LLM clients see unambiguous parameter types. Handlers SHALL continue to accept omitted optional parameters.

#### Scenario: Optional boolean advertised as plain boolean
- **WHEN** an MCP client lists tools and inspects `get_oidc_client_logo`
- **THEN** the `light` property's schema is `"type": "boolean"` (not a type array), and `light` is absent from `required`

#### Scenario: No nullable type arrays anywhere
- **WHEN** an MCP client lists tools with all safety tiers enabled
- **THEN** no property in any advertised input schema carries an array-valued `"type"`
