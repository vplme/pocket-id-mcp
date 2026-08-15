# server-runtime

## ADDED Requirements

### Requirement: Transport selection
The server SHALL support two MCP transports selected at startup via `POCKET_ID_MCP_TRANSPORT`: `stdio` (default) and `http` (Streamable HTTP per the MCP 2026-07-28 revision, or the newest revision the pinned rmcp SDK supports). In either transport the server SHALL advertise its identity (name `pocket-id-mcp`, crate version) and its registered tools.

#### Scenario: Default stdio operation
- **WHEN** an MCP client launches the binary with no transport configured
- **THEN** the server serves MCP over stdio and lists its registered tools

#### Scenario: HTTP transport opt-in
- **WHEN** the server starts with `POCKET_ID_MCP_TRANSPORT=http` and valid HTTP-mode configuration
- **THEN** it serves Streamable HTTP on `POCKET_ID_MCP_HTTP_BIND` (default `127.0.0.1:8756`) and does not serve stdio

### Requirement: OAuth 2.1 resource server in HTTP mode
In HTTP mode the server SHALL act as an OAuth 2.1 protected resource: publish RFC 9728 protected resource metadata at `/.well-known/oauth-protected-resource` naming `POCKET_ID_MCP_PUBLIC_URL` as the resource identifier and the configured authorization server (`POCKET_ID_MCP_OAUTH_ISSUER`, defaulting to the Pocket ID instance) as the issuer; challenge unauthenticated requests with `401` and a `WWW-Authenticate` header referencing that metadata; and validate every request's bearer token (signature against the issuer's JWKS as resolved from its discovery metadata, issuer, expiry, and audience binding to the resource identifier). Any OIDC-discovery-compliant authorization server (e.g., Keycloak) SHALL be usable as the issuer. The server SHALL remain client-registration agnostic: it implements no registration mechanism, and CIMD, DCR, or pre-registered clients all work as the issuer permits. The server SHALL NOT forward MCP-client bearer tokens to the Pocket ID API; upstream calls use the configured API key only.

#### Scenario: External authorization server
- **WHEN** `POCKET_ID_MCP_OAUTH_ISSUER` points at a non-Pocket-ID OIDC-compliant issuer (e.g., a Keycloak realm)
- **THEN** the server resolves that issuer's discovery metadata and JWKS and validates tokens against it, with no Pocket-ID-specific assumptions

#### Scenario: Unauthenticated request challenged
- **WHEN** an HTTP request arrives without a bearer token
- **THEN** the server responds `401` with a `WWW-Authenticate` header pointing at its protected resource metadata

#### Scenario: Wrong-audience token rejected
- **WHEN** a request presents a valid Pocket ID token whose audience is not the server's resource identifier
- **THEN** the server rejects it with `401` and does not process the MCP request

#### Scenario: No token passthrough
- **WHEN** an authenticated MCP request triggers a Pocket ID API call
- **THEN** the upstream request carries only `X-API-KEY`, never the client's bearer token

### Requirement: Group-based admission in HTTP mode
When `POCKET_ID_MCP_ALLOWED_GROUPS` is set, the server SHALL admit only tokens whose groups claim (claim name from `POCKET_ID_MCP_GROUPS_CLAIM`, default `groups`) intersects the configured list; when unset, any token that passes validation is admitted (with admission expected to be constrained at the authorization server, e.g., Pocket ID allowed user groups or Keycloak client policies).

#### Scenario: Group restriction enforced
- **WHEN** `POCKET_ID_MCP_ALLOWED_GROUPS=admins` and a validated token's groups claim lacks `admins`
- **THEN** the request is rejected with `403`

### Requirement: Workflow prompts
The server SHALL expose a curated set of MCP prompts encoding common multi-step workflows over the primitive tools, including at minimum: OIDC client onboarding, user access audit, and instance health check. Prompts SHALL respect safety tiers (a prompt requiring write-tier tools is not exposed in read-only mode).

#### Scenario: Prompts listed
- **WHEN** an MCP client lists prompts
- **THEN** the workflow prompts are returned with descriptions and arguments

### Requirement: Structured tool output schemas
Tools returning structured JSON SHALL declare an output schema derived from their response types, so MCP clients can validate and chain results.

#### Scenario: Output schema advertised
- **WHEN** an MCP client lists tools
- **THEN** tools with structured responses include an `outputSchema` in their definitions

### Requirement: Environment-based configuration
The server SHALL read its configuration exclusively from environment variables: `POCKET_ID_URL` (required), `POCKET_ID_API_KEY` (required), `POCKET_ID_MCP_READ_ONLY` (optional, default false), `POCKET_ID_MCP_ALLOW_DANGEROUS` (optional, default false), `POCKET_ID_MCP_TRANSPORT` (optional, default `stdio`), and in HTTP mode `POCKET_ID_MCP_HTTP_BIND` (optional), `POCKET_ID_MCP_PUBLIC_URL` (required), `POCKET_ID_MCP_OAUTH_ISSUER` (optional, default `POCKET_ID_URL`), `POCKET_ID_MCP_ALLOWED_GROUPS` (optional), `POCKET_ID_MCP_GROUPS_CLAIM` (optional, default `groups`).

#### Scenario: Missing required configuration
- **WHEN** the server starts without `POCKET_ID_URL` or `POCKET_ID_API_KEY` set, or in HTTP mode without `POCKET_ID_MCP_PUBLIC_URL`
- **THEN** it exits non-zero before serving, printing a message naming the missing variable

#### Scenario: Boolean flags parsed leniently
- **WHEN** `POCKET_ID_MCP_READ_ONLY` is set to `true`, `1`, or `yes` (case-insensitive)
- **THEN** read-only mode is enabled; any other value (or unset) leaves it disabled

### Requirement: Startup connectivity validation
The server SHALL validate connectivity and credentials against the configured Pocket ID instance at startup by calling `GET /api/version/current`; in HTTP mode it SHALL additionally fetch the configured issuer's discovery document and JWKS before accepting requests.

#### Scenario: Unreachable instance or invalid key
- **WHEN** the validation request fails (network error or 401/403)
- **THEN** the server exits non-zero with a message distinguishing "cannot reach instance" from "API key rejected"

### Requirement: Safety-tier tool registration
The server SHALL classify every tool into exactly one tier — `read`, `write`, or `dangerous` — and register only the tools permitted by configuration: `read` tools always; `write` tools unless read-only mode is enabled; `dangerous` tools only when `POCKET_ID_MCP_ALLOW_DANGEROUS` is enabled.

#### Scenario: Read-only mode
- **WHEN** the server starts with `POCKET_ID_MCP_READ_ONLY=true`
- **THEN** the MCP tool list contains no `write`-tier or `dangerous`-tier tools

#### Scenario: Dangerous tools hidden by default
- **WHEN** the server starts with default configuration
- **THEN** tools classified `dangerous` (one-time access token/email minting, user deletion, passkey deletion, API-key revocation, signup-token creation) are absent from the tool list

#### Scenario: Dangerous tools opted in
- **WHEN** the server starts with `POCKET_ID_MCP_ALLOW_DANGEROUS=true` and read-only mode disabled
- **THEN** all tiers are registered
