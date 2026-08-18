# server-runtime (delta)

## ADDED Requirements

### Requirement: Strict tool input parameters
Tool input handling SHALL be closed-world. A tool call whose arguments contain a top-level key not declared in the tool's input schema SHALL be rejected as a JSON-RPC invalid-params error (`-32602`) naming the unknown field, and the handler SHALL NOT run. Every advertised tool input schema SHALL declare `additionalProperties: false` at its top level, matching the enforced behavior. Parameters with a closed set of valid values known at build time (sort direction; audit-log location) SHALL be advertised as string enums in the input schema and SHALL reject values outside the set with an error naming the valid values; optional enum parameters SHALL be advertised as a plain inline enum schema (no `anyOf`/`$ref` indirection), consistent with the plain-types requirement for optional parameters.

#### Scenario: Unknown parameter rejected instead of silently dropped
- **WHEN** `list_all_audit_logs` is called with a REST-style `filters` object instead of the tool's flat parameters
- **THEN** the call fails with an invalid-params error naming the unknown `filters` field, and no upstream request is made

#### Scenario: Strictness advertised in the schema
- **WHEN** an MCP client lists tools with all safety tiers enabled
- **THEN** every tool's input schema declares `additionalProperties: false` at the top level

#### Scenario: Closed value set advertised as enum and enforced
- **WHEN** a client inspects `list_all_audit_logs` and then calls it with `location` set to a value other than `internal` or `external`
- **THEN** the advertised `location` schema is a string enum of exactly those values, and the call fails with an error naming the valid variants

#### Scenario: Optional enum renders as a typed input
- **WHEN** an MCP client lists tools and inspects a list tool's `sort_direction` property
- **THEN** its schema is an inline `{"type": "string", "enum": ["asc", "desc"]}` with no `anyOf` or `$ref`, and `sort_direction` is absent from `required`
