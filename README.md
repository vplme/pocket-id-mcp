# pocket-id-mcp

[![CI](https://github.com/vplme/pocket-id-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/vplme/pocket-id-mcp/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/vplme/pocket-id-mcp/coverage/badge.json)](https://github.com/vplme/pocket-id-mcp/actions/workflows/coverage.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

An MCP (Model Context Protocol) server for [Pocket ID](https://pocket-id.org) — the self-hosted, passkey-first OIDC identity provider.

It exposes the complete Pocket ID REST API (103 operations) as **84 curated MCP tools** so AI assistants like Claude can manage your instance conversationally: users, groups, OIDC clients, custom claims, passkeys, branding images, audit logs, API keys, and SCIM provisioning — with safety tiers around destructive operations.

- **Single static binary** (Rust, [rmcp](https://github.com/modelcontextprotocol/rust-sdk)), fast startup, tiny footprint
- **Two transports**: stdio (default) and Streamable HTTP secured with OAuth 2.1
- **Safety tiers**: read / write / dangerous — dangerous operations (user deletion, passkey deletion, login-token minting, API-key revocation) are invisible unless explicitly enabled
- **Images round-trip**: upload branding images from a file or URL, and *see* the result — image GETs return real MCP image content
- **Tested against the real thing**: a CI test asserts every operation in the vendored upstream spec is either mapped to a tool or excluded with a documented reason; a live suite drives the built binary over MCP against a real Pocket ID container and verifies each mutation through Pocket ID's own REST API; a weekly workflow diffs against upstream for API drift

## Installation

Prebuilt binaries for macOS (arm64/x64) and Linux (arm64/x64) are on [GitHub Releases](../../releases). Or:

```sh
cargo install --git https://github.com/vplme/pocket-id-mcp   # from source
docker pull ghcr.io/vplme/pocket-id-mcp:latest               # container
```

Create an API key in Pocket ID under **Settings → API Keys** (admin account required).

## Claude Code / Claude Desktop setup

Claude Code:

```sh
claude mcp add pocket-id \
  --env POCKET_ID_URL=https://id.example.com \
  --env POCKET_ID_API_KEY=your-api-key \
  -- pocket-id-mcp
```

Claude Desktop (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "pocket-id": {
      "command": "/path/to/pocket-id-mcp",
      "env": {
        "POCKET_ID_URL": "https://id.example.com",
        "POCKET_ID_API_KEY": "your-api-key"
      }
    }
  }
}
```

On startup the server validates connectivity and the API key against `GET /api/version/current` and fails fast with a clear message if either is wrong.

## Environment variables

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `POCKET_ID_URL` | yes | — | Base URL of your Pocket ID instance |
| `POCKET_ID_API_KEY` | yes | — | Admin API key (sent as `X-API-KEY`) |
| `POCKET_ID_MCP_READ_ONLY` | no | `false` | `true`/`1`/`yes`: register only read tools |
| `POCKET_ID_MCP_ALLOW_DANGEROUS` | no | `false` | `true`/`1`/`yes`: also register dangerous tools |
| `POCKET_ID_MCP_TRANSPORT` | no | `stdio` | `stdio` or `http` |
| `POCKET_ID_MCP_HTTP_BIND` | no | `127.0.0.1:8756` | HTTP mode: bind address |
| `POCKET_ID_MCP_HTTP_AUTH` | no | `oauth` | HTTP mode: `oauth`, `token` (static bearer secret), or `none` (loopback only) |
| `POCKET_ID_MCP_PUBLIC_URL` | `oauth` mode | `http://localhost:<port>` in other modes | HTTP mode: external URL of the MCP endpoint — the OAuth resource identifier (e.g. `https://mcp.example.com/mcp`) |
| `POCKET_ID_MCP_OAUTH_ISSUER` | no | `POCKET_ID_URL` | `oauth` mode only: OAuth authorization server (any OIDC-compliant issuer) |
| `POCKET_ID_MCP_ALLOWED_GROUPS` | no | — | `oauth` mode only: comma-separated groups; tokens must carry at least one |
| `POCKET_ID_MCP_GROUPS_CLAIM` | no | `groups` | `oauth` mode only: claim name holding the group list |
| `POCKET_ID_MCP_HTTP_TOKEN` | `token` mode | — | `token` mode only: the shared bearer secret clients must present |
| `POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK` | no | `false` | `none` mode: allow a non-loopback bind (dangerous — see below) |

OAuth-only variables are **rejected at startup** in `token`/`none` modes (and `POCKET_ID_MCP_HTTP_TOKEN` outside `token` mode), so a misconfigured setup fails loudly instead of silently skipping enforcement you thought you had.

## Safety tiers

Every tool is classified into exactly one tier. Gated tools are **not registered at all** — invisible to the assistant, no error-retry loops.

| Tier | Contents | Enabled |
|---|---|---|
| **read** (34 tools) | All GETs, introspection, previews | always |
| **write** (42 tools) | Create/update, image uploads, LDAP/SCIM sync, group deletes | unless `POCKET_ID_MCP_READ_ONLY` |
| **dangerous** (8 tools) | User deletion, passkey deletion, one-time login token/email minting, signup-token create/delete, API-key revocation | only with `POCKET_ID_MCP_ALLOW_DANGEROUS` |

> The API key itself is all-or-nothing admin power. Tiers protect against assistant mistakes, not against host compromise — prefer short-expiry keys.

## HTTP mode (remote / shared use)

In HTTP mode the server speaks Streamable HTTP at `/mcp`. Authentication is selected with `POCKET_ID_MCP_HTTP_AUTH`:

- **`oauth`** (default) — OAuth 2.1 protected resource; the right choice for anything reachable beyond your machine. Detailed below.
- **`token`** — a static shared bearer secret; good default for local HTTP (one long-running server shared by local MCP clients) and shared machines.
- **`none`** — no authentication; only accepted on a loopback bind. For quick local testing.

### Local HTTP quick start

```sh
export POCKET_ID_MCP_TRANSPORT=http
export POCKET_ID_MCP_HTTP_AUTH=token
export POCKET_ID_MCP_HTTP_TOKEN=$(openssl rand -hex 32)
pocket-id-mcp   # serves http://127.0.0.1:8756/mcp
```

Point clients at `http://localhost:8756/mcp` with header `Authorization: Bearer <token>`. Or, for throwaway testing with e.g. MCP Inspector, `POCKET_ID_MCP_HTTP_AUTH=none` drops the header requirement entirely.

> **⚠️ `none` mode**: whoever can reach the port wields your admin API key — the server therefore refuses to start unauthenticated on a non-loopback bind unless you set `POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK=true`. Don't, outside of an isolated network you fully trust. Note that loopback is not a full boundary either: any local process/user can call the server, and only Host-header validation (always active) stands between a malicious webpage and DNS-rebinding its way in. Prefer `token` mode.

### OAuth mode

The server acts as an **OAuth 2.1 protected resource**: it publishes RFC 9728 metadata at `/.well-known/oauth-protected-resource`, challenges unauthenticated requests with `WWW-Authenticate`, and validates every request's bearer token (signature via the issuer's JWKS, issuer, expiry, and audience = `POCKET_ID_MCP_PUBLIC_URL`). Client bearer tokens are **never** forwarded to Pocket ID — upstream calls use only the API key.

```sh
export POCKET_ID_MCP_TRANSPORT=http
export POCKET_ID_MCP_PUBLIC_URL=https://mcp.example.com/mcp
export POCKET_ID_MCP_ALLOWED_GROUPS=admins   # strongly recommended
pocket-id-mcp
```

**Authorization server.** Defaults to your Pocket ID instance itself; any OIDC-discovery-compliant issuer (Keycloak, Authentik, …) works via `POCKET_ID_MCP_OAUTH_ISSUER`.

**Register the resource (Pocket ID as issuer).** MCP clients request tokens bound to this server's identity (RFC 8707 `resource` parameter). Pocket ID mints such audience-bound tokens through its **API definitions** feature, so one-time setup is required:

1. **Settings → API definitions → Add** — name it e.g. `pocket-id-mcp`, resource = your `POCKET_ID_MCP_PUBLIC_URL` (exact match, e.g. `https://mcp.example.com/mcp`), and add one permission (e.g. key `use`).
2. On the OAuth client (below), grant that permission under **user-delegated API access**.

Without this, token requests carrying `resource` are denied by Pocket ID, and tokens minted without it are audienced to the client ID — which this server rejects.

**Client registration.** MCP clients that support CIMD (Client ID Metadata Documents) self-register with Pocket ID: add the client's metadata URL pattern to Pocket ID's **CIMD URL allowlist** (the document must be hosted at a public https URL — Pocket ID refuses private addresses), then grant the materialized client API access as above. For other clients, pre-register a public OAuth client:

1. **Settings → OIDC Clients → Add** — name it e.g. `MCP clients`.
2. Mark it **public** (no secret) and leave **PKCE** enabled.
3. Add your MCP client's redirect URIs (e.g. `http://localhost:PORT/callback` for Claude Code, or the callback documented by your client).
4. Optionally restrict the client to an admins group under **Allowed user groups**.

All of this can also be done through this server's own tools (`create_api_definition`, `set_api_definition_permissions`, `update_client_api_access`, `create_oidc_client`) — see `scripts/e2e-oauth.py` for a complete scripted example.

**Who gets in.** Anyone who completes the OAuth flow wields the server's admin API key. Restrict admission twice: at the issuer (allowed user groups on the OAuth client) *and* at the server (`POCKET_ID_MCP_ALLOWED_GROUPS=admins`, claim name configurable via `POCKET_ID_MCP_GROUPS_CLAIM`).

**TLS / reverse proxy.** The server binds `127.0.0.1` and terminates no TLS. Put it behind your reverse proxy:

```caddy
mcp.example.com {
    reverse_proxy 127.0.0.1:8756
}
```

Docker sidecar next to Pocket ID:

```yaml
services:
  pocket-id-mcp:
    image: ghcr.io/vplme/pocket-id-mcp:latest
    environment:
      POCKET_ID_URL: https://id.example.com
      POCKET_ID_API_KEY: ${POCKET_ID_API_KEY}
      POCKET_ID_MCP_TRANSPORT: http
      POCKET_ID_MCP_HTTP_BIND: 0.0.0.0:8756
      POCKET_ID_MCP_PUBLIC_URL: https://mcp.example.com/mcp
      POCKET_ID_MCP_ALLOWED_GROUPS: admins
    ports:
      - "127.0.0.1:8756:8756"
```

## Prompts

Three workflow prompts encode common multi-step operations (tier-aware — write prompts disappear in read-only mode):

- **onboard-oidc-client** — create a client, restrict groups, hand over the shown-once secret and endpoints
- **audit-user-access** — profile → groups → grants → passkeys → recent audit activity
- **instance-health-check** — health, version vs latest, risky configuration review

## Tool catalog

<details>
<summary>All 84 tools by area (click to expand)</summary>

**Identity: Users**

| Tool | Tier |
|---|---|
| `list_users` | read |
| `get_user` | read |
| `get_current_user` | read |
| `list_user_groups_of_user` | read |
| `get_user_profile_picture` | read |
| `create_user` | write |
| `update_user` | write |
| `update_current_user` | write |
| `delete_user` | dangerous |
| `update_user_profile_picture` | write |
| `reset_user_profile_picture` | write |
| `update_current_user_profile_picture` | write |
| `reset_current_user_profile_picture` | write |
| `send_current_user_email_verification` | write |
| `verify_current_user_email` | write |
| `set_user_groups` | write |

**Identity: Groups**

| Tool | Tier |
|---|---|
| `list_groups` | read |
| `get_group` | read |
| `create_group` | write |
| `update_group` | write |
| `delete_group` | write |
| `set_group_users` | write |

**Identity: Custom Claims**

| Tool | Tier |
|---|---|
| `get_custom_claim_suggestions` | read |
| `update_user_custom_claims` | write |
| `update_group_custom_claims` | write |

**Identity: Passkeys**

| Tool | Tier |
|---|---|
| `list_user_passkeys` | read |
| `delete_user_passkey` | dangerous |

**Identity: Signup And One-Time Access**

| Tool | Tier |
|---|---|
| `list_signup_tokens` | read |
| `create_signup_token` | dangerous |
| `delete_signup_token` | dangerous |
| `create_one_time_access_token` | dangerous |
| `send_one_time_access_email` | dangerous |
| `request_one_time_access_email` | dangerous |

**Oidc: Clients**

| Tool | Tier |
|---|---|
| `list_oidc_clients` | read |
| `get_oidc_client` | read |
| `create_oidc_client` | write |
| `update_oidc_client` | write |
| `delete_oidc_client` | write |
| `create_oidc_client_secret` | write |
| `update_oidc_client_allowed_groups` | write |
| `get_oidc_client_metadata` | read |
| `refresh_oidc_client_metadata` | write |
| `preview_oidc_client_for_user` | read |
| `get_oidc_client_logo` | read |
| `update_oidc_client_logo` | write |
| `delete_oidc_client_logo` | write |
| `set_group_allowed_oidc_clients` | write |

**Oidc: Tokens And Grants**

| Tool | Tier |
|---|---|
| `introspect_token` | read |
| `list_user_authorized_clients` | read |
| `list_my_authorized_clients` | read |
| `revoke_my_authorized_client` | write |
| `list_my_accessible_clients` | read |

**Oidc: Api Definitions And Access**

| Tool | Tier |
|---|---|
| `get_client_api_access` | read |
| `update_client_api_access` | write |
| `list_api_definitions` | read |
| `get_api_definition` | read |
| `create_api_definition` | write |
| `update_api_definition` | write |
| `delete_api_definition` | write |
| `set_api_definition_permissions` | write |

**Admin: Application Images**

| Tool | Tier |
|---|---|
| `get_application_image` | read |
| `update_application_image` | write |
| `delete_application_image` | write |

**Admin: Configuration**

| Tool | Tier |
|---|---|
| `get_public_application_configuration` | read |
| `get_all_application_configuration` | read |
| `update_application_configuration` | write |
| `sync_ldap` | write |
| `send_test_email` | write |

**Admin: Audit Logs**

| Tool | Tier |
|---|---|
| `list_my_audit_logs` | read |
| `list_all_audit_logs` | read |
| `list_audit_log_client_names` | read |
| `list_audit_log_users` | read |

**Admin: Api Keys**

| Tool | Tier |
|---|---|
| `list_api_keys` | read |
| `create_api_key` | write |
| `renew_api_key` | write |
| `revoke_api_key` | dangerous |

**Admin: Scim**

| Tool | Tier |
|---|---|
| `create_scim_service_provider` | write |
| `update_scim_service_provider` | write |
| `delete_scim_service_provider` | write |
| `sync_scim_service_provider` | write |
| `get_client_scim_service_provider` | read |

**Admin: Status**

| Tool | Tier |
|---|---|
| `get_current_version` | read |
| `get_latest_version` | read |
| `health_check` | read |

</details>

## API coverage

`spec/swagger.yaml` vendors the upstream API spec (currently Pocket ID v2.13.0). A test fails if any operation is neither mapped to a tool nor listed in `spec/exclusions.toml` with a reason (excluded: browser signup/setup flows, device-login endpoints, OIDC protocol endpoints, one-time token redemption). A weekly GitHub Actions job diffs upstream and opens a tracking issue on drift.

## Development

```sh
cargo test                                   # hermetic: unit, HTTP-auth, tier, conformance, spec-coverage tests
POCKET_ID_LIVE=1 cargo test --test live      # live: Gherkin features against a real Pocket ID
POCKET_ID_LIVE=1 cargo test --test live -- --tags @oidc      # one area; --name "secret" for one scenario
```

The live suite is written as Gherkin features (`tests/features/*.feature`, run by [cucumber-rs](https://github.com/cucumber-rs/cucumber) from `tests/live/`). It starts a pinned Pocket ID container via Docker (`pocket-id-mcp-live`, left running afterwards for inspection), bootstraps the first admin and API keys through the one-time `/api/signup/setup` flow, then for every scenario spawns the real `pocket-id-mcp` binary over stdio, drives it through an MCP client, and verifies the effect by reading Pocket ID back directly over REST — never through the server's own client. Write-only values are proven by use (a minted client secret must authenticate to `/api/oidc/introspect`; a revoked API key must stop authenticating). For example:

```gherkin
Scenario: Updating a client persists every field
  Given a confidential OIDC client "{unique}"
  When I update that client with:
    | name               | {unique}-renamed               |
    | callbackURLs       | https://new.example.com/cb     |
    | skipConsent        | true                           |
  Then Pocket ID's record of that client has:
    | name               | {unique}-renamed               |
    | callbackURLs       | https://new.example.com/cb     |
    | skipConsent        | true                           |
```

Data-table cells are typed by the tool's advertised input schema, so a misspelled parameter fails loudly. 34 scenarios exercise 68 of the 84 tools; the rest need infrastructure the suite does not provide (an SMTP sink, LDAP, a SCIM endpoint, a public CIMD document, a real passkey or consent flow). The suite also pins observed upstream contracts: Pocket ID refuses API-key-authenticated API-key creation/renewal, and token introspection authenticates with OAuth client credentials only, so `create_api_key`, `renew_api_key` and `introspect_token` cannot succeed under this server's API-key auth. Knobs:

| Variable | Purpose |
|---|---|
| `POCKET_ID_LIVE=1` | Opt in (the binary exits early otherwise, so plain `cargo test` stays offline) |
| `POCKET_ID_LIVE_URL` + `POCKET_ID_LIVE_API_KEY` | Test against an existing instance instead of Docker (admin API key; `@needs-bootstrap` scenarios are skipped) |
| `POCKET_ID_LIVE_IMAGE` | Container image (default `ghcr.io/pocket-id/pocket-id:v2.13.0`, matching the vendored spec) |
| `POCKET_ID_LIVE_PORT` | Host port for the container (default `1431`) |

The suite runs in CI on every pull request (`live` job). `scripts/e2e-oauth.py` additionally exercises the full OAuth 2.1 + PKCE flow in HTTP mode and stays a manual driver (needs `cloudflared`).

**Coverage.** The badge is line coverage of `src/` from the hermetic suites *plus* the live suite — the `pocket-id-mcp` binary the scenarios spawn is instrumented too, so tool bodies that only run against a real Pocket ID count. It is computed by the `Coverage` workflow (`cargo-llvm-cov`) and published to the `coverage` branch on every push to `main`. Locally:

```sh
cargo llvm-cov --no-report test                                # hermetic
POCKET_ID_LIVE=1 cargo llvm-cov --no-report test --test live    # live
cargo llvm-cov report --open                                   # HTML report (or --summary-only)
```

## License

[MIT](LICENSE)
