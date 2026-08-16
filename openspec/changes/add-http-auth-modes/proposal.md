# Proposal: add-http-auth-modes

## Why

The HTTP transport currently mandates OAuth 2.1: the server refuses to boot unless issuer discovery and JWKS fetch succeed, and every request must carry a validated bearer token. That is right for network-exposed deployments, but it makes purely local HTTP use (one long-running server shared by local MCP clients, quick testing with MCP Inspector or curl) needlessly heavy — and the only escape hatch today is the stdio transport, which forces one spawned process per client.

## What Changes

- Introduce an HTTP auth mode setting `POCKET_ID_MCP_HTTP_AUTH` with three values:
  - `oauth` (default) — exactly today's behavior; no migration needed for existing deployments.
  - `token` — a static shared bearer secret from `POCKET_ID_MCP_HTTP_TOKEN`, compared in constant time; no issuer, discovery, or JWKS involved.
  - `none` — no authentication middleware at all; permitted only when the bind address is loopback, unless explicitly overridden.
- In `none` mode, refuse to start when `POCKET_ID_MCP_HTTP_BIND` is non-loopback unless `POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK=true` is set (the server fronts an admin API key, so an open port means full IdP admin).
- In `token` and `none` modes:
  - `POCKET_ID_MCP_PUBLIC_URL` becomes optional (defaults to `http://localhost:<bind port>`; it only exists as the OAuth resource identifier).
  - The OAuth-only variables (`POCKET_ID_MCP_OAUTH_ISSUER`, `POCKET_ID_MCP_ALLOWED_GROUPS`, `POCKET_ID_MCP_GROUPS_CLAIM`) are rejected at startup if set, so nobody silently loses group enforcement they thought they had.
  - The `/.well-known/oauth-protected-resource` metadata route is not served, and `401` responses carry no OAuth `WWW-Authenticate` challenge that would send clients on a dead-end OAuth dance.
  - The startup OAuth issuer/JWKS probe is skipped (Pocket ID API connectivity validation still runs).
- DNS-rebinding protection (allowed-hosts validation) remains active in all modes — it is the only request-level defense left in `none` mode.
- README documents the new modes and their security trade-offs.

No breaking changes: absent `POCKET_ID_MCP_HTTP_AUTH`, behavior is identical to today.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `server-runtime`: The "OAuth 2.1 resource server in HTTP mode" requirement becomes conditional on the new `oauth` auth mode (still the default); new requirements cover static-token auth, unauthenticated loopback mode with its non-loopback guard, and rejection of OAuth-only configuration in non-OAuth modes; the "Environment-based configuration" and "Startup connectivity validation" requirements gain the new variables and mode-dependent behavior.

## Impact

- `src/config.rs`: new `HttpAuthMode` enum on `HttpConfig`; conditional requiredness of `POCKET_ID_MCP_PUBLIC_URL`; validation of mode/variable combinations and the loopback guard.
- `src/http/mod.rs`: middleware selection per mode; metadata route only in `oauth` mode; skip `Authenticator` construction/init outside `oauth` mode.
- `src/http/auth.rs`: unchanged for `oauth`; `token` mode gets a small constant-time comparison path (likely a separate lightweight middleware rather than touching `Authenticator`).
- `tests/`: config-validation cases and router-level integration tests per mode.
- `README.md`: configuration table and security guidance.
- Dependencies: possibly `subtle` (or a hand-rolled constant-time compare) — no other new crates.
