# Tasks: add-pocket-id-mcp-server

## 1. Project scaffolding

- [x] 1.1 Initialize Cargo binary crate `pocket-id-mcp` with rmcp, tokio, reqwest, serde, serde_json, schemars dependencies; set edition, lints, and release profile (LTO, stripped)
- [x] 1.2 Vendor upstream `swagger.yaml` into `spec/swagger.yaml` and record the Pocket ID version it corresponds to
- [x] 1.3 Set up `Cargo.toml` metadata (description, license, repository) and a basic `rustfmt`/clippy-clean baseline

## 2. Configuration and runtime

- [x] 2.1 Implement config module: parse `POCKET_ID_URL`, `POCKET_ID_API_KEY`, `POCKET_ID_MCP_READ_ONLY`, `POCKET_ID_MCP_ALLOW_DANGEROUS` with lenient boolean parsing and fail-fast errors naming missing variables
- [x] 2.2 Implement safety-tier model (`read`/`write`/`dangerous`) and tier-filtered tool registration
- [x] 2.3 Implement transport selection (`POCKET_ID_MCP_TRANSPORT`) and stdio server bootstrap with rmcp, server info, and startup connectivity validation via `GET /api/version/current` distinguishing unreachable vs unauthorized
- [x] 2.4 Unit tests for config parsing (incl. HTTP-mode required vars) and tier filtering
- [x] 2.5 Spike: confirm the pinned rmcp release's Streamable HTTP support and which MCP revision it speaks (target 2026-07-28 stateless); confirm Pocket ID can issue audience-bound tokens for a custom resource identifier — record findings in design.md

## 2b. HTTP transport and OAuth

- [x] 2b.1 Implement Streamable HTTP serving via rmcp/axum on `POCKET_ID_MCP_HTTP_BIND`, sharing the transport-agnostic tool surface with stdio
- [x] 2b.2 Serve RFC 9728 protected resource metadata at `/.well-known/oauth-protected-resource` (resource identifier = `POCKET_ID_MCP_PUBLIC_URL`, authorization server = `POCKET_ID_MCP_OAUTH_ISSUER`, defaulting to the Pocket ID instance) and 401 + `WWW-Authenticate` challenges referencing it
- [x] 2b.3 Implement bearer-token validation middleware: issuer discovery (OIDC/RFC 8414), JWKS fetch/cache/refresh, signature + `iss` + `exp` + audience checks; introspection fallback (RFC 7662) for issuers without audience-bound JWTs
- [x] 2b.4 Implement `POCKET_ID_MCP_ALLOWED_GROUPS` admission check with configurable claim name (`POCKET_ID_MCP_GROUPS_CLAIM`, 403 on mismatch); ensure client bearer tokens are never forwarded upstream
- [x] 2b.5 Startup validation for HTTP mode: OIDC discovery + JWKS reachable, `POCKET_ID_MCP_PUBLIC_URL` present and well-formed
- [x] 2b.6 Integration tests: 401 challenge shape, wrong-audience rejection, group admission, happy-path tool call with a valid token (mock AS or live dev instance); verify against at least one non-Pocket-ID issuer (e.g., Keycloak container)

## 3. HTTP client layer

- [x] 3.1 Implement `client` module: base-URL joining, `X-API-KEY` injection, JSON request/response helpers, typed error mapping (status + upstream message, no key leakage)
- [x] 3.2 Implement multipart upload helper (from `file_path` or fetched HTTPS `url`, content-type inference, exactly-one-source validation)
- [x] 3.3 Implement binary download helper preserving bytes + content type; MCP image content block construction with >2 MB temp-file fallback
- [x] 3.4 Define DTOs for request/response bodies used by the tool surface (users, groups, OIDC clients, config, audit, API keys, SCIM, claims, signup, versions)
- [x] 3.5 Unit tests for error mapping and upload-source validation (mock server via `wiremock` or similar)

## 4. Identity tools

- [x] 4.1 User tools: list/get/create/update (id and me), delete (dangerous), profile-picture get/update/reset (id and me)
- [x] 4.2 Group tools: list/get/create/update/delete, set group members, set user's groups
- [x] 4.3 Custom claims tools: suggestions, set for user, set for group
- [x] 4.4 Passkey tools: list user passkeys, delete passkey (dangerous)
- [x] 4.5 Onboarding/access tools: signup tokens list (read) + create/delete (dangerous), one-time access token/email minting (dangerous)

## 5. OIDC tools

- [x] 5.1 OIDC client CRUD tools: list/get/create/update/delete, metadata get/refresh, preview for user
- [x] 5.2 Client secret regeneration tool with shown-once warning in description
- [x] 5.3 Allowed-user-groups tool (client side) and allowed-oidc-clients tool (group side)
- [x] 5.4 Client logo tools (get/update/delete) using shared upload/download helpers
- [x] 5.5 Introspection tool and authorized/accessible client tools (per-user and me variants, revoke for me)
- [x] 5.6 API definitions and permissions tools; client API access get/update

## 6. Admin tools

- [x] 6.1 Application image tools with `image_type` enum + `light` validation (3 tools covering 12 operations)
- [x] 6.2 Application configuration tools: public read, admin read-all, update, LDAP sync, test email
- [x] 6.3 Audit log tools with filter/pagination inputs and filter-lookup endpoints
- [x] 6.4 API key tools: list/create/renew (write), revoke (dangerous), shown-once warning on create
- [x] 6.5 SCIM service provider tools: create/update/delete/sync, per-client read
- [x] 6.6 Version and health tools
- [x] 6.7 Declare output schemas (via schemars response types) on all tools with structured JSON responses
- [x] 6.8 Implement workflow prompts (OIDC client onboarding, user access audit, instance health check) with safety-tier awareness

## 7. Coverage and drift enforcement

- [x] 7.1 Implement spec coverage test: parse vendored `swagger.yaml`, assert every operation is mapped to a tool or listed in `spec/exclusions.toml` with a reason
- [x] 7.2 Populate exclusion list for intentionally unexposed operations (browser signup flows, all device-login endpoints, well-known endpoints) with documented reasons
- [x] 7.3 GitHub Actions: build + clippy + test workflow on push/PR
- [x] 7.4 GitHub Actions: weekly + manual spec-drift workflow that diffs upstream swagger against vendored copy and opens/updates a single tracking issue

## 8. Verification and docs

- [x] 8.1 Manual end-to-end pass against a live Pocket ID instance (docker-compose dev instance): user/group CRUD, OIDC client creation with secret, image round-trip with visual verification, audit log query
- [x] 8.2 Verify safety tiers end-to-end: default hides dangerous tools, read-only hides writes
- [x] 8.2b End-to-end HTTP mode pass: connect a real MCP client through the OAuth authorization-code flow (PKCE) against Pocket ID — via CIMD self-registration, plus the pre-registered-client fallback — and exercise tools over Streamable HTTP
- [x] 8.3 Write README: installation, Claude Code/Desktop config snippet, environment variables, safety tiers, tool catalog table, HTTP mode setup (OIDC client pre-registration walkthrough, reverse-proxy/TLS guidance, allowed-groups posture)
- [x] 8.4 GitHub Release workflow producing prebuilt binaries (macOS arm64/x64, Linux arm64/x64) and publishing a container image to GHCR (with an HTTP-mode sidecar example in the README)
