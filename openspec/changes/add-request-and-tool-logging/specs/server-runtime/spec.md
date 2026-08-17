## MODIFIED Requirements

### Requirement: Environment-based configuration
The server SHALL read its configuration exclusively from environment variables: `POCKET_ID_URL` (required), `POCKET_ID_API_KEY` (required), `POCKET_ID_MCP_READ_ONLY` (optional, default false), `POCKET_ID_MCP_ALLOW_DANGEROUS` (optional, default false), `POCKET_ID_MCP_TRANSPORT` (optional, default `stdio`), `POCKET_ID_MCP_LOG_FORMAT` (optional, `text` or `json`, defaulting by whether the log stream is a terminal), and in HTTP mode `POCKET_ID_MCP_HTTP_BIND` (optional), `POCKET_ID_MCP_PUBLIC_URL` (required), `POCKET_ID_MCP_OAUTH_ISSUER` (optional, default `POCKET_ID_URL`), `POCKET_ID_MCP_ALLOWED_GROUPS` (optional), `POCKET_ID_MCP_GROUPS_CLAIM` (optional, default `groups`).

#### Scenario: Missing required configuration
- **WHEN** the server starts without `POCKET_ID_URL` or `POCKET_ID_API_KEY` set, or in HTTP mode without `POCKET_ID_MCP_PUBLIC_URL`
- **THEN** it exits non-zero before serving, printing a message naming the missing variable

#### Scenario: Boolean flags parsed leniently
- **WHEN** `POCKET_ID_MCP_READ_ONLY` is set to `true`, `1`, or `yes` (case-insensitive)
- **THEN** read-only mode is enabled; any other value (or unset) leaves it disabled

#### Scenario: Log format rejected when invalid
- **WHEN** `POCKET_ID_MCP_LOG_FORMAT` is set to a value other than `text` or `json`
- **THEN** the server exits non-zero before serving, printing a message naming the variable and the accepted values

### Requirement: Safety-tier tool registration
The server SHALL classify every tool into exactly one tier — `read`, `write`, or `dangerous` — and register only the tools permitted by configuration: `read` tools always; `write` tools unless read-only mode is enabled; `dangerous` tools only when `POCKET_ID_MCP_ALLOW_DANGEROUS` is enabled. Each registered tool's tier SHALL be resolvable at dispatch time from its tool name, so that tier can be attributed in logs without duplicating the tier classification.

#### Scenario: Read-only mode
- **WHEN** the server starts with `POCKET_ID_MCP_READ_ONLY=true`
- **THEN** the MCP tool list contains no `write`-tier or `dangerous`-tier tools

#### Scenario: Dangerous tools hidden by default
- **WHEN** the server starts with default configuration
- **THEN** tools classified `dangerous` (one-time access token/email minting, user deletion, passkey deletion, API-key revocation, signup-token creation) are absent from the tool list

#### Scenario: Dangerous tools opted in
- **WHEN** the server starts with `POCKET_ID_MCP_ALLOW_DANGEROUS=true` and read-only mode disabled
- **THEN** all tiers are registered

#### Scenario: Every registered tool has a resolvable tier
- **WHEN** the server dispatches a call to any registered tool
- **THEN** a tier is resolved for that tool name from the single tier classification used for registration
