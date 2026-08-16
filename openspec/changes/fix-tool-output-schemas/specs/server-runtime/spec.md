# server-runtime (delta)

## MODIFIED Requirements

### Requirement: Structured tool output schemas
Tools returning structured JSON SHALL declare an output schema derived from their response types, so MCP clients can validate and chain results. Every declared output schema SHALL be object-rooted (`"type": "object"`), as required by the MCP tool schema; results whose natural shape is an array or non-object value SHALL be wrapped in a `{"result": <value>}` envelope, with the envelope reflected in both the output schema and the returned structured content.

#### Scenario: Output schema advertised
- **WHEN** an MCP client lists tools
- **THEN** tools with structured responses include an `outputSchema` in their definitions

#### Scenario: All output schemas are object-rooted
- **WHEN** an MCP client that validates tool definitions (e.g. Claude Code) fetches the tool list
- **THEN** every declared `outputSchema` has root `"type": "object"` and the full list is accepted

#### Scenario: Array result wrapped in envelope
- **WHEN** a tool whose response is a list (e.g. `list_user_passkeys`) is called
- **THEN** the structured content is `{"result": [...]}` conforming to the declared object-rooted schema
