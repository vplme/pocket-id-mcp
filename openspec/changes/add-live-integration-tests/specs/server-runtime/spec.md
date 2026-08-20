# server-runtime (delta)

## ADDED Requirements

### Requirement: Live verification against a real Pocket ID
The test suite SHALL include an opt-in live suite that drives the built server binary over MCP against a real Pocket ID instance and verifies each tool's effect independently through Pocket ID's REST API, not through the server's own client. The suite SHALL run in CI against a pinned Pocket ID release matching the vendored API spec.

#### Scenario: Mutation observable in Pocket ID
- **WHEN** a write tool (e.g. `create_oidc_client`) is called with parameters through the MCP client
- **THEN** reading the resource back directly from Pocket ID's REST API returns those parameters

#### Scenario: Write-only values proven by use
- **WHEN** a tool mints a value that cannot be read back (client secret, API key)
- **THEN** the suite proves Pocket ID holds it by authenticating with it (and that the superseded/revoked value no longer authenticates)

#### Scenario: Hermetic default
- **WHEN** `cargo test` runs without the live opt-in
- **THEN** the live suite compiles but exits with a notice, and all other tests pass
