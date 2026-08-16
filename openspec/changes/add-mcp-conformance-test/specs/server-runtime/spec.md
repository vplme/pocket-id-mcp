# server-runtime (delta)

## ADDED Requirements

### Requirement: MCP protocol schema conformance
The server's advertised tool definitions SHALL validate against the official MCP JSON Schema for the pinned protocol revision (the newest revision the pinned rmcp SDK supports), vendored in the repository. The test suite SHALL perform this validation over the full tool surface (all tiers enabled) and report violations by JSON path.

#### Scenario: Tool list validates against the vendored schema
- **WHEN** the test suite serializes the complete registered tool list as a `ListToolsResult` and validates it against the vendored MCP schema
- **THEN** validation passes with zero violations

#### Scenario: Nonconforming definition caught at test time
- **WHEN** a tool declares a definition violating the MCP `Tool` type (e.g. a non-object `outputSchema` root or a boolean property schema)
- **THEN** the conformance test fails, reporting the JSON path of each violation
