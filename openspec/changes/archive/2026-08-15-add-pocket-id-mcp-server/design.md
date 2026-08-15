# Design: add-pocket-id-mcp-server

## Context

Pocket ID is a self-hosted, passkey-first OIDC identity provider (Go backend). It exposes a REST API (~103 operations, 74 paths, Swagger 2.0 spec served at `https://pocket-id.org/swagger.yaml`) authenticated by a single admin-scoped `X-API-KEY` header. There is no official MCP integration. This project builds a standalone Rust MCP server that maps the full API to a curated tool surface. The repository is greenfield.

Key upstream facts established during exploration:

- All five application images (logo, favicon, background, email, default profile picture) share one shape: `GET` → binary, `PUT` → `multipart/form-data` with a `file` field, `DELETE` → reset. Logo adds a `?light=` boolean for light/dark variants.
- The API key is all-or-nothing admin power; the server must supply its own safety tiers.
- `POST /api/users/{id}/one-time-access-token` mints a login token for any user — effectively impersonation.

## Goals / Non-Goals

**Goals:**

- Full functional coverage of the Pocket ID REST API through MCP tools.
- Two transports: stdio for local single-user use, Streamable HTTP (MCP revision 2026-07-28) for remote/shared use, secured with OAuth 2.1 against a configurable authorization server — Pocket ID itself by default, but any OIDC-discovery-compliant issuer (e.g., Keycloak) works.
- Efficient: small static binary, low memory, fast startup, no runtime interpreter.
- Safe by default: destructive/impersonation operations gated; optional read-only mode.
- Ergonomic tool surface: ~55–65 tools with clear names, rich descriptions, and enums instead of 103 mechanical mappings.
- Detect upstream API drift automatically in CI.

**Non-Goals:**

- Multi-tenant HTTP deployments (one server instance serves one Pocket ID instance with one upstream API key; per-user upstream permission mapping is out of scope).
- Built-in TLS termination — HTTP mode expects a reverse proxy for TLS; the server binds localhost by default.
- Image processing (resizing, format conversion, favicon generation) — the assistant orchestrates; the server just transfers bytes.
- Composite/workflow tools (e.g., a single `setup_oidc_client` doing create + restrict + fetch-secret) — primitives are enough for assistants to chain; the workflow knowledge ships as MCP prompts instead (D6b).
- Interactive/browser-driven flows: signup UI, WebAuthn ceremonies, and all device-login endpoints (they exist for interactive CLI login journeys an assistant should not drive — excluded in v1 with documented reasons). Signup-token management endpoints are still covered.
- Supporting Pocket ID versions older than the vendored spec.

## Decisions

### D1: Rust with the official `rmcp` SDK

Chosen for efficiency (single static binary, minimal footprint) and safety (type system catches schema/serialization mistakes across a large tool surface). `rmcp` is the official Rust SDK with `#[tool]` macros and schemars-based input schemas. Alternatives: TypeScript SDK (most mature, but node runtime + npx distribution), Go (matches upstream, but user preference and safety story favor Rust).

### D1b: Dual transport — stdio default, Streamable HTTP opt-in

`POCKET_ID_MCP_TRANSPORT=stdio` (default) keeps the zero-friction local path. `=http` serves Streamable HTTP per MCP revision 2026-07-28, which is stateless (no sessions, no `Mcp-Session-Id`, no initialize handshake) — a natural fit for an axum service where every request is independently authenticated. The tool surface, safety tiers, and client layer are transport-agnostic; transport is selected at startup, not served simultaneously. Practical constraint: the pinned rmcp release dictates which protocol revision the wire actually speaks; if rmcp lags 2026-07-28, ship the newest revision it supports and track the upgrade.

### D2: Hand-written tool layer over a hand-written thin client, driven by the vendored spec — no code generation step

The swagger spec (Swagger 2.0) is vendored into the repo as the source of truth for *coverage accounting*, not as input to a codegen pipeline. Rationale: OpenAPI→Rust generators handle Swagger 2.0 + multipart poorly, and generated code fights the enum-collapsing design. Instead: a thin `client` module (reqwest wrapper: auth header injection, JSON/multipart/binary helpers, typed error mapping) plus hand-written DTOs for the subset of definitions tools actually surface. A `coverage.rs` test asserts every operation in the vendored spec is mapped to a tool (or explicitly listed as excluded with a reason) — this replaces codegen as the "nothing missed" guarantee.

Alternatives considered: full codegen from spec (fragile, poor ergonomics), macro-generating tools from an overlay DSL (clever but opaque; revisit if maintenance burden proves real).

### D3: Enum-collapsing convention for structurally identical endpoints

Where endpoints differ only by a path segment, collapse into one tool with an enum parameter:

- 12 application-image ops → `get_application_image` / `update_application_image` / `delete_application_image` with `image_type: logo|favicon|background|email|default_profile_picture` and optional `light: bool` (logo only; reject for other types).
- Audit-log filter endpoints → parameters on `list_audit_logs` companions.
- `users/me` vs `users/{id}` variants stay separate tools — "current user" vs "admin acting on user" are different intents with different safety profiles.

Target surface: ~55–65 tools. Naming: `verb_resource` snake_case (`list_users`, `create_oidc_client`, `update_application_image`).

### D4: Binary transfer model

- **Uploads** (`update_application_image`, profile pictures, client logos): input is either `file_path` (read from local disk — valid because stdio servers run on the user's machine) or `url` (server fetches over HTTPS then re-uploads). Exactly one must be provided.
- **Downloads** (image GETs): returned as MCP image content blocks (base64 + mime type from the response `Content-Type`), so the assistant can visually verify branding changes. Oversized responses (> ~2 MB) fall back to writing a temp file and returning its path.

### D5: Safety tiers enforced in the server

Three tiers, controlled by environment config:

| Tier | Contents | Default |
|---|---|---|
| read | all GETs, introspection, previews | always on |
| write | create/update ops, image uploads, LDAP sync | on; disabled by `POCKET_ID_MCP_READ_ONLY=true` |
| dangerous | one-time access token/email minting, user deletion, passkey deletion, API-key revocation, signup-token creation | off; enabled by `POCKET_ID_MCP_ALLOW_DANGEROUS=true` |

Gated tools are not registered at startup (invisible to the client) rather than registered-but-erroring — keeps context clean and prevents retry loops.

### D5b: OAuth 2.1 in HTTP mode — MCP server as resource server, configurable authorization server

The MCP authorization spec forbids token passthrough: tokens presented to the MCP server MUST be audience-bound to it and MUST NOT be forwarded upstream. So OAuth answers "may this person use this MCP server?" while the admin `X-API-KEY` remains the server's own upstream credential — two separate trust links. The authorization server is any OIDC-discovery-compliant issuer, configured via `POCKET_ID_MCP_OAUTH_ISSUER`; when unset it defaults to the Pocket ID instance itself (the natural single-stack setup), but Keycloak, Authentik, or any other compliant AS works identically:

```
MCP client ──(OAuth 2.1 bearer, aud=MCP server)──▶ pocket-id-mcp ──(X-API-KEY)──▶ Pocket ID API
                     ▲                                    │
                     └────── authorization code flow ─────┴──▶ authorization server
                                                    (issuer: Pocket ID by default,
                                                     or Keycloak/other OIDC-compliant AS)
```

Mechanics:

- **Discovery**: serve RFC 9728 protected resource metadata at `/.well-known/oauth-protected-resource`, listing `POCKET_ID_MCP_PUBLIC_URL` as the resource identifier and the configured issuer as the authorization server. Unauthenticated requests get `401` with a `WWW-Authenticate` header pointing at that metadata. The issuer's own metadata is resolved via OIDC discovery / RFC 8414 — never hardcoded paths.
- **Token validation**: validate JWTs locally against the issuer's JWKS (URI taken from its discovery document, cached with refresh) checking signature, `iss`, `exp`, and audience matching the resource identifier (RFC 8707 resource indicators). Fall back to token introspection (RFC 7662, endpoint from discovery metadata) for issuers that mint opaque or non-audience-bound tokens.
- **Client registration**: registration is strictly between the MCP client and the issuer — the server is registration-method agnostic by design and implements nothing registration-related. Revision 2026-07-28 prefers Client ID Metadata Documents (CIMD, with DCR deprecated), and Pocket ID supports CIMD, so the default single-stack setup is fully self-service for CIMD-capable MCP clients. For issuers or clients without CIMD, DCR (where the issuer offers it, e.g. Keycloak) or admin pre-registration of a public OAuth client (always permitted by the spec) work identically from the server's perspective. Document the pre-registration client settings (public client + PKCE, redirect URIs for common MCP clients) as the universal fallback.
- **Authorization (who gets in)**: first line is the issuer's own access restrictions on that OAuth client (e.g., Pocket ID allowed-user-groups, Keycloak client access policies); additionally `POCKET_ID_MCP_ALLOWED_GROUPS` (comma-separated) makes the server verify a groups claim in the token, with the claim name configurable via `POCKET_ID_MCP_GROUPS_CLAIM` (default `groups`) since issuers differ (Keycloak often uses realm roles or a mapped claim). Anyone admitted wields the server's admin API key — restricting admission to an admins group is the documented default posture.

### D6: Configuration

Environment variables only (MCP-conventional). Core: `POCKET_ID_URL` (required), `POCKET_ID_API_KEY` (required), `POCKET_ID_MCP_READ_ONLY`, `POCKET_ID_MCP_ALLOW_DANGEROUS`. Transport: `POCKET_ID_MCP_TRANSPORT` (`stdio` default | `http`); HTTP mode adds `POCKET_ID_MCP_HTTP_BIND` (default `127.0.0.1:8756`), `POCKET_ID_MCP_PUBLIC_URL` (required in HTTP mode — the OAuth resource identifier), `POCKET_ID_MCP_OAUTH_ISSUER` (optional, defaults to `POCKET_ID_URL`), `POCKET_ID_MCP_ALLOWED_GROUPS` (optional), `POCKET_ID_MCP_GROUPS_CLAIM` (optional, default `groups`). On startup the server validates connectivity with `GET /api/version/current` (plus issuer discovery + JWKS fetch in HTTP mode) and fails fast with a clear message on bad URL/key.

### D6b: MCP prompts and output schemas

Ship a small curated set of MCP prompts encoding the multi-step workflows the tool surface deliberately keeps primitive (see Non-Goals): e.g., `onboard-oidc-client` (create client → restrict groups → fetch secret), `audit-user-access` (user → groups → authorized clients → audit logs), `instance-health-check` (version vs latest + health). Prompts carry the orchestration knowledge without composite tools. Tools with structured JSON responses declare `outputSchema` (nearly free via schemars) so clients can validate and chain results.

### D7: Spec-drift CI

A GitHub Actions workflow (scheduled weekly + manual dispatch) downloads upstream `swagger.yaml`, diffs its operation set against the vendored copy, and opens/updates an issue listing added/removed/changed operations. The coverage test (D2) then enforces that a vendored-spec update forces tool-surface reconciliation.

### D8: Distribution

Prebuilt binaries via GitHub Releases (macOS arm64/x64, Linux arm64/x64, statically linked where practical) plus a container image published to GHCR — the container matters because the Pocket ID audience is Docker-heavy self-hosters and HTTP mode is a natural sidecar next to Pocket ID itself. crates.io publication deferred.

## Spike findings (task 2.5, resolved 2026-08-15)

**rmcp Streamable HTTP / protocol revision.** Pinned `rmcp = 3.1.2`. Its
`ProtocolVersion::KNOWN_VERSIONS` includes `2026-07-28` and the SDK negotiates it when a
client requests it (`STANDARD_HEADERS` gate for SEP-2243 headers is implemented), but the
SDK default (`LATEST`) is `2025-11-25` and the Streamable HTTP server still runs
session-managed (`LocalSessionManager`, `Mcp-Session-Id` issued on initialize) rather than
the fully stateless 2026-07-28 mode. Per D1b's fallback clause we ship what the pinned SDK
speaks: the server negotiates down/up per client (verified end-to-end: a 2025-06-18 client
gets 2025-06-18). Transport setup is isolated in `src/http/mod.rs` so the upgrade to
stateless mode is contained. Note: rmcp enforces Host-header allowlisting (DNS-rebinding
protection); the server allows loopback, the bind address, and the `POCKET_ID_MCP_PUBLIC_URL`
host.

**Audience-bound tokens from Pocket ID.** Verified live (task 8.2b, 2026-08-15):
Pocket ID v2.13 implements RFC 8707 resource indicators via its **API definitions**
feature — the resource identifier must be registered as an API definition (with at least
one permission) and the OAuth client granted a user-delegated permission on it
(`PUT /api/api-access/{clientId}`); tokens minted with `resource=<POCKET_ID_MCP_PUBLIC_URL>`
then carry it in `aud`. Without the `resource` parameter, tokens are audienced to the
client ID and this server correctly rejects them (verified). The resource server enforces
`aud == POCKET_ID_MCP_PUBLIC_URL` on JWTs regardless of issuer. Implemented fallback:
opaque tokens are introspected only when the issuer *is* the Pocket ID instance, via
`POST /api/oidc/introspect` authenticated with the server's own API key (generic RFC 7662
introspection against external issuers would require resource-server client credentials the
configuration deliberately does not include, so opaque tokens from external issuers are
rejected with a clear message telling the client to present a JWT). Introspection responses
that carry an `aud` are audience-checked the same way.

**CIMD specifics (verified live).** Pocket ID's CIMD fetcher blocks private/loopback
addresses (SSRF protection), so the metadata document must live at a genuinely public
https URL; the `cimdUrlAllowlist` app-config value is a JSON-encoded array of URL patterns
(e.g. `["https://client.example.com/*"]`). CIMD client IDs (full URLs) cannot travel as
raw path segments in Pocket ID's admin API — the convention is `~<base64url(id)>`,
implemented in `tools::client_seg` and applied to every client-scoped tool so CIMD clients
are manageable (including granting them API access for audience-bound tokens). Both the
pre-registered-client and CIMD flows were driven end-to-end by `scripts/e2e-oauth.py`.

## Risks / Trade-offs

- [Upstream API drift between releases] → vendored spec + weekly drift workflow + coverage test make drift visible and reconciliation mandatory.
- [Admin API key in env var is full-power regardless of server tiers] → document clearly; tiers protect against assistant mistakes, not against host compromise. Recommend short-expiry keys.
- [`rmcp` SDK is younger than the TS SDK] → pin version; the surface used (stdio server, tool macros) is its most-exercised path.
- [Hand-written DTOs can typo-drift from spec] → DTOs are deserialized with `deny_unknown_fields` off (forward-compatible) and covered by the coverage test plus integration tests against a live dev instance (docker-compose) where feasible.
- [Large tool count may bloat some MCP clients' context] → mitigated by curated descriptions kept terse and by tier gating trimming the registered set; if needed later, add an opt-in "core tools only" mode.
- [HTTP mode exposes admin-level capability to the network] → localhost bind by default, TLS via reverse proxy documented as required for non-local binds, allowed-groups posture documented; OAuth admission is authentication + coarse authorization only.
- [rmcp may not yet implement MCP revision 2026-07-28 (stateless Streamable HTTP)] → pin rmcp, ship the newest revision it supports, isolate transport setup in one module so the upgrade is contained.
- [Pocket ID may not mint audience-bound access tokens for a custom resource identifier] → verify early (spike task); fallback is introspection-based validation, still spec-compliant at the resource server.

## Open Questions

- Whether Pocket ID's tokens carry a groups claim by default or need a custom claim configured — affects `POCKET_ID_MCP_ALLOWED_GROUPS` documentation only; resolved during end-to-end verification.
