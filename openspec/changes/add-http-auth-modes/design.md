# Design: add-http-auth-modes

## Context

HTTP mode is hard-wired to OAuth 2.1: `serve()` in `src/http/mod.rs` unconditionally constructs an `Authenticator`, fails startup if issuer discovery/JWKS fetch fails, and layers `auth_middleware` over `/mcp`. `POCKET_ID_MCP_PUBLIC_URL` is required solely to serve as the OAuth resource identifier (audience). The server holds a Pocket ID **admin API key**, so any relaxation of HTTP auth is a direct path to full IdP admin for whoever can reach the port. DNS-rebinding protection already exists via rmcp's allowed-hosts check (`allowed_hosts()` in `src/http/mod.rs`).

## Goals / Non-Goals

**Goals:**
- Let a locally-run HTTP server operate without an OAuth issuer: either fully unauthenticated (loopback-guarded) or with a static shared bearer token.
- Keep `oauth` the default with byte-for-byte-identical behavior; zero migration for existing deployments.
- Fail loudly on incoherent configuration (OAuth-only vars set in non-OAuth modes; `none` on a non-loopback bind).

**Non-Goals:**
- Multiple simultaneous auth modes, per-route auth, or mTLS.
- Rate limiting or audit logging of unauthenticated access.
- Changing the stdio transport or the upstream API-key model in any way.

## Decisions

### 1. Mode enum, not boolean flags

`POCKET_ID_MCP_HTTP_AUTH` ∈ {`oauth` (default), `token`, `none`}, parsed into `enum HttpAuthMode` carried on `HttpConfig`. Alternative considered: a `POCKET_ID_MCP_NO_AUTH` boolean — rejected because it cannot express the `token` middle tier and invites double-negative configs.

`HttpConfig` restructures so invalid states are unrepresentable:

```rust
pub enum HttpAuthMode {
    OAuth { issuer: String, allowed_groups: Option<Vec<String>>, groups_claim: String },
    StaticToken { token: String },
    None,
}
pub struct HttpConfig {
    pub bind: String,
    pub public_url: String, // always resolved; defaulted in non-oauth modes
    pub auth: HttpAuthMode,
}
```

Alternative: keep flat `HttpConfig` fields plus a mode discriminant — rejected; the enum makes "issuer exists only in oauth mode" a type-level fact and simplifies both validation and `serve()`.

### 2. Config validation matrix (all at startup, in `Config::from_vars`)

| Mode | `PUBLIC_URL` | `OAUTH_ISSUER` / `ALLOWED_GROUPS` / `GROUPS_CLAIM` | `HTTP_TOKEN` | Bind |
|---|---|---|---|---|
| `oauth` | required | allowed (current defaults) | rejected if set | any |
| `token` | optional → `http://localhost:<port>` | **rejected if set** | required, non-empty | any |
| `none` | optional → `http://localhost:<port>` | **rejected if set** | rejected if set | loopback only, unless override |

Rejecting (rather than ignoring) OAuth-only vars in non-OAuth modes is deliberate: silently ignoring `ALLOWED_GROUPS` would let someone believe group admission is enforced when it is not. Same logic for a stray `HTTP_TOKEN` under `oauth`/`none`.

Loopback guard: parse the host part of `bind`; accept `127.0.0.0/8` literals, `::1`, and `localhost`. Anything else requires `POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK=true` (lenient-bool). The deliberately verbose variable name is the "I understand" affordance. Note the guard keys off the bind address, not `public_url`.

### 3. Middleware selection in `serve()`/`build_router()`

- `oauth`: unchanged — `Authenticator::init()` startup probe, `auth_middleware`, both `/.well-known/oauth-protected-resource` routes.
- `token`: no `Authenticator` at all. A small middleware extracts the bearer token exactly like today and compares against the configured secret with a constant-time comparison. Failure → plain `401` with `WWW-Authenticate: Bearer error="invalid_token"` but **no** `resource_metadata` pointer and no metadata routes (nothing should invite an OAuth dance that cannot succeed).
- `none`: no auth middleware, no metadata routes.
- All modes: rmcp `allowed_hosts` stays exactly as-is — in `none` mode the Host-header check is the only remaining request-level defense against DNS rebinding from a browser.

`HttpState` becomes mode-shaped too (e.g., `enum` or `Option<OAuthState>`); the metadata handler only exists in the oauth branch.

### 4. Constant-time comparison without a new dependency

Hand-roll the compare (length check folded into a byte-wise `|` accumulator over max-length iteration, or `subtle` if we'd rather not argue about it in review). Preference: use the tiny well-audited `subtle` crate (`ConstantTimeEq`) — it is a one-line dependency, and hand-rolled constant-time code is a classic review trap. This is the only dependency addition.

### 5. Startup validation

Non-OAuth modes skip issuer discovery/JWKS entirely; the existing `GET /api/version/current` Pocket ID connectivity check runs in every mode unchanged. Startup log line states the active auth mode explicitly (`auth_mode = "none"` etc.) so a misdeployment is visible in logs.

## Risks / Trade-offs

- [`none` + user overrides the loopback guard and exposes an admin-key proxy] → The override variable name spells out "UNAUTHENTICATED_NON_LOOPBACK"; README carries an explicit warning box; startup logs the mode loudly.
- [Loopback bind is not full local security — any local process/user can call the server] → Documented; `token` mode is recommended in README for shared machines.
- [DNS rebinding against a no-auth localhost server] → rmcp allowed-hosts (Host-header validation) remains active in all modes; covered by an integration test asserting a spoofed Host is rejected in `none` mode.
- [Config restructure ripples through existing tests] → `HttpConfig` is internal; test updates are mechanical and the oauth-mode defaults tests must remain green unchanged in expectation.
- [Static token in env var can leak via process listing/env dumps] → Same exposure class as the already-required `POCKET_ID_API_KEY`; no new secret-handling machinery warranted.

## Migration Plan

None needed: default mode is `oauth` with identical behavior. Rollback = unset the new variables.

## Open Questions

- None blocking. (If Docker docs later want a `token`-mode example, that's README-only follow-up.)
