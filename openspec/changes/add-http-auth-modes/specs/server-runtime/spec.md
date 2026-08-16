# server-runtime (delta)

## ADDED Requirements

### Requirement: HTTP auth mode selection
In HTTP mode the server SHALL read `POCKET_ID_MCP_HTTP_AUTH` (optional, default `oauth`) selecting one of three authentication modes: `oauth` (OAuth 2.1 protected resource, identical to prior behavior), `token` (static shared bearer secret), or `none` (no authentication). Any other value SHALL cause a non-zero exit naming the variable. The server SHALL log the active auth mode at startup. In `token` and `none` modes the server SHALL reject startup if any OAuth-only variable (`POCKET_ID_MCP_OAUTH_ISSUER`, `POCKET_ID_MCP_ALLOWED_GROUPS`, `POCKET_ID_MCP_GROUPS_CLAIM`) is set, naming the conflicting variable, and SHALL NOT serve `/.well-known/oauth-protected-resource` metadata.

#### Scenario: Default remains OAuth
- **WHEN** the server starts in HTTP mode without `POCKET_ID_MCP_HTTP_AUTH`
- **THEN** it behaves exactly as an OAuth 2.1 protected resource, requiring issuer discovery at startup and bearer validation per request

#### Scenario: OAuth-only variable rejected in non-OAuth mode
- **WHEN** the server starts with `POCKET_ID_MCP_HTTP_AUTH=none` and `POCKET_ID_MCP_ALLOWED_GROUPS=admins`
- **THEN** it exits non-zero before serving with a message naming `POCKET_ID_MCP_ALLOWED_GROUPS` as incompatible with the selected auth mode

#### Scenario: Invalid mode rejected
- **WHEN** `POCKET_ID_MCP_HTTP_AUTH` is set to an unrecognized value
- **THEN** the server exits non-zero naming the variable and the accepted values

### Requirement: Static-token authentication in HTTP mode
When `POCKET_ID_MCP_HTTP_AUTH=token`, the server SHALL require a non-empty `POCKET_ID_MCP_HTTP_TOKEN` and SHALL admit a request only when its `Authorization: Bearer` credential equals the configured secret, using a constant-time comparison. Requests with a missing or mismatched credential SHALL receive `401` with a `WWW-Authenticate: Bearer` challenge that carries no `resource_metadata` reference. No OAuth issuer discovery, JWKS fetch, or token introspection SHALL occur in this mode. `POCKET_ID_MCP_HTTP_TOKEN` set in any other auth mode SHALL cause a non-zero exit.

#### Scenario: Matching token admitted
- **WHEN** a request presents `Authorization: Bearer <configured secret>`
- **THEN** the MCP request is processed

#### Scenario: Wrong token rejected
- **WHEN** a request presents a bearer credential that differs from the configured secret
- **THEN** the server responds `401` and does not process the MCP request

#### Scenario: Token mode without a token
- **WHEN** the server starts with `POCKET_ID_MCP_HTTP_AUTH=token` and no `POCKET_ID_MCP_HTTP_TOKEN`
- **THEN** it exits non-zero naming the missing variable

### Requirement: Unauthenticated HTTP mode with loopback guard
When `POCKET_ID_MCP_HTTP_AUTH=none`, the server SHALL serve `/mcp` without authentication middleware, but SHALL refuse to start when the bind address host is not loopback (`127.0.0.0/8` literal, `::1`, or `localhost`) unless `POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK` is truthy. Host-header (DNS-rebinding) validation SHALL remain active in this mode.

#### Scenario: Unauthenticated request served on loopback
- **WHEN** the server runs with `POCKET_ID_MCP_HTTP_AUTH=none` bound to `127.0.0.1` and a request arrives without any `Authorization` header
- **THEN** the MCP request is processed

#### Scenario: Non-loopback bind refused
- **WHEN** the server starts with `POCKET_ID_MCP_HTTP_AUTH=none` and `POCKET_ID_MCP_HTTP_BIND=0.0.0.0:8756` without the override variable
- **THEN** it exits non-zero explaining that unauthenticated mode requires a loopback bind or the explicit override

#### Scenario: Explicit override honored
- **WHEN** the same non-loopback configuration additionally sets `POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK=true`
- **THEN** the server starts and serves unauthenticated requests

#### Scenario: DNS-rebinding protection retained
- **WHEN** a request arrives in `none` mode with a `Host` header not in the allowed set
- **THEN** the request is rejected and no MCP processing occurs

## MODIFIED Requirements

### Requirement: OAuth 2.1 resource server in HTTP mode
When `POCKET_ID_MCP_HTTP_AUTH` is `oauth` (the default), the server SHALL act as an OAuth 2.1 protected resource: publish RFC 9728 protected resource metadata at `/.well-known/oauth-protected-resource` naming `POCKET_ID_MCP_PUBLIC_URL` as the resource identifier and the configured authorization server (`POCKET_ID_MCP_OAUTH_ISSUER`, defaulting to the Pocket ID instance) as the issuer; challenge unauthenticated requests with `401` and a `WWW-Authenticate` header referencing that metadata; and validate every request's bearer token (signature against the issuer's JWKS as resolved from its discovery metadata, issuer, expiry, and audience binding to the resource identifier). Any OIDC-discovery-compliant authorization server (e.g., Keycloak) SHALL be usable as the issuer. The server SHALL remain client-registration agnostic: it implements no registration mechanism, and CIMD, DCR, or pre-registered clients all work as the issuer permits. The server SHALL NOT forward MCP-client bearer tokens to the Pocket ID API; upstream calls use the configured API key only.

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

### Requirement: Environment-based configuration
The server SHALL read its configuration exclusively from environment variables: `POCKET_ID_URL` (required), `POCKET_ID_API_KEY` (required), `POCKET_ID_MCP_READ_ONLY` (optional, default false), `POCKET_ID_MCP_ALLOW_DANGEROUS` (optional, default false), `POCKET_ID_MCP_TRANSPORT` (optional, default `stdio`), and in HTTP mode `POCKET_ID_MCP_HTTP_BIND` (optional), `POCKET_ID_MCP_HTTP_AUTH` (optional, default `oauth`), `POCKET_ID_MCP_PUBLIC_URL` (required in `oauth` mode; optional otherwise, defaulting to `http://localhost:<bind port>`), `POCKET_ID_MCP_OAUTH_ISSUER` (optional in `oauth` mode, default `POCKET_ID_URL`; rejected otherwise), `POCKET_ID_MCP_ALLOWED_GROUPS` (optional in `oauth` mode; rejected otherwise), `POCKET_ID_MCP_GROUPS_CLAIM` (optional in `oauth` mode, default `groups`; rejected otherwise), `POCKET_ID_MCP_HTTP_TOKEN` (required in `token` mode; rejected otherwise), and `POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK` (optional, default false, meaningful only in `none` mode).

#### Scenario: Missing required configuration
- **WHEN** the server starts without `POCKET_ID_URL` or `POCKET_ID_API_KEY` set, or in HTTP `oauth` mode without `POCKET_ID_MCP_PUBLIC_URL`
- **THEN** it exits non-zero before serving, printing a message naming the missing variable

#### Scenario: Boolean flags parsed leniently
- **WHEN** `POCKET_ID_MCP_READ_ONLY` is set to `true`, `1`, or `yes` (case-insensitive)
- **THEN** read-only mode is enabled; any other value (or unset) leaves it disabled

#### Scenario: Public URL defaulted in non-OAuth modes
- **WHEN** the server starts with `POCKET_ID_MCP_HTTP_AUTH=none` or `token` and no `POCKET_ID_MCP_PUBLIC_URL`
- **THEN** it starts successfully using `http://localhost:<bind port>` as its base URL

### Requirement: Startup connectivity validation
The server SHALL validate connectivity and credentials against the configured Pocket ID instance at startup by calling `GET /api/version/current`; in HTTP `oauth` mode it SHALL additionally fetch the configured issuer's discovery document and JWKS before accepting requests. In HTTP `token` and `none` modes no issuer contact SHALL occur.

#### Scenario: Unreachable instance or invalid key
- **WHEN** the validation request fails (network error or 401/403)
- **THEN** the server exits non-zero with a message distinguishing "cannot reach instance" from "API key rejected"

#### Scenario: Non-OAuth mode starts without issuer reachability
- **WHEN** the server starts with `POCKET_ID_MCP_HTTP_AUTH=none` while the Pocket ID instance serves its API but not OIDC discovery
- **THEN** the server starts successfully, having performed only the API connectivity check
