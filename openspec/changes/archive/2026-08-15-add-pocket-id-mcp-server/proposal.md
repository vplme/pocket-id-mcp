# Proposal: add-pocket-id-mcp-server

## Why

Managing a self-hosted Pocket ID instance today means clicking through the admin UI or hand-crafting `curl` calls — tedious for routine work (rotating branding images, wiring up a new OIDC client, auditing sign-ins) and impossible to delegate to an AI assistant. An MCP server exposing the full Pocket ID REST API lets Claude and other MCP clients do this work conversationally, with safety rails the raw admin API key does not provide.

## What Changes

- New Rust binary crate `pocket-id-mcp`: an MCP server (rmcp SDK) covering the entire Pocket ID REST API (~103 operations across 74 paths in upstream `swagger.yaml`).
- Two transports: stdio (default, zero-config beyond env vars) and Streamable HTTP (opt-in), the latter secured with OAuth 2.1 per the current MCP authorization spec — the server acts as an OAuth resource server against a configurable authorization server (Pocket ID itself by default; any OIDC-compliant issuer such as Keycloak works).
- Structurally identical endpoints are collapsed into single tools with enums (e.g., 12 application-image operations become 3 tools with an image-type enum and a `light` variant flag), landing at roughly 55–65 tools with no loss of API coverage.
- Binary handling: image/file uploads accept a local `file_path` or a `url` (server fetches and re-uploads); image GETs return MCP image content blocks so the assistant can visually verify results.
- Upstream authentication via `X-API-KEY` header, configured through environment variables (`POCKET_ID_URL`, `POCKET_ID_API_KEY`). In HTTP mode, MCP clients additionally authenticate to the server with OAuth 2.1 bearer tokens from the configured issuer — Pocket ID by default, or any OIDC-compliant authorization server (MCP spec revision 2026-07-28: RFC 9728 protected resource metadata, audience-validated tokens, no token passthrough upstream).
- Safety tiers: read-only mode switch, and dangerous operations (one-time-access-token minting, user deletion, passkey revocation) disabled unless explicitly enabled by config.
- HTTP client layer generated/derived from a vendored copy of upstream `swagger.yaml`, with a hand-curated tool metadata overlay (names, descriptions, enum collapsing).
- CI job that diffs the vendored swagger spec against upstream to detect API drift on new Pocket ID releases.
- Curated MCP prompts encoding common workflows (OIDC client onboarding, user access audit, instance health check); tools with structured responses declare output schemas.
- Distribution via GitHub: prebuilt binaries on Releases and a container image on GHCR.

## Capabilities

### New Capabilities

- `server-runtime`: MCP server lifecycle — transport selection (stdio default, Streamable HTTP opt-in), OAuth 2.1 resource-server behavior in HTTP mode against a configurable authorization server (Pocket ID by default), configuration from environment, safety-tier enforcement (read-only mode, dangerous-op gating), startup validation against the Pocket ID instance.
- `api-client`: Typed HTTP client for the Pocket ID REST API derived from the vendored swagger spec — API-key auth, error mapping, multipart uploads, binary downloads.
- `identity-tools`: MCP tools for users, user groups, group membership, custom claims, signup tokens, and passkey (WebAuthn credential) management.
- `oidc-tools`: MCP tools for OIDC clients (CRUD, secrets, logos, allowed groups, preview), client API access, token introspection, and authorized-client management.
- `admin-tools`: MCP tools for application configuration, application images (branding), audit logs, API keys, API/permission definitions, SCIM service providers, LDAP sync, version/health checks.
- `spec-drift-ci`: CI workflow that compares the vendored swagger spec with upstream Pocket ID and fails/notifies when coverage drifts.

### Modified Capabilities

_None — greenfield project._

## Impact

- New Cargo workspace/crate at repo root; new dependencies: `rmcp`, `tokio`, `reqwest`, `serde`, `schemars` (tool schemas), image/multipart support; HTTP mode adds an HTTP server stack (axum via rmcp's streamable-HTTP feature) and JWT validation (`jsonwebtoken` or equivalent against Pocket ID's JWKS).
- Targets MCP specification revision 2026-07-28 (stateless Streamable HTTP), constrained by what the pinned rmcp release implements.
- Vendored `swagger.yaml` committed to the repo as the generation/coverage source of truth.
- New GitHub Actions workflows (build/test, spec-drift check).
- No existing code affected — repository currently contains only scaffolding.
