# Proposal: add-live-integration-tests

## Why

Nothing in `cargo test` ever talks to a real Pocket ID: the HTTP-auth, tier, conformance and coverage tests all run against wiremock or in-process routers, so the only proof that a tool actually does what it says against the real API was `scripts/e2e-live.py` — a manual Python driver, not run in CI, that also required a pre-existing `scripts/apikey.txt`. Upstream behaviour already drifts in ways mocks cannot catch: Pocket ID 2.14 removed `POST /api/oidc/clients/{id}/secret` (now `/secrets`), and Pocket ID refuses API-key authentication for API-key creation/renewal altogether — neither was visible before pointing the real binary at a real instance.

## What Changes

- Add a Rust live integration suite, `tests/live/` (one cargo test target, `live`), that starts a **pinned** Pocket ID container via the Docker CLI, bootstraps the first admin and API keys through the one-time `/api/signup/setup` flow, and for every scenario spawns the **real `pocket-id-mcp` binary** over stdio (rmcp client, `CARGO_BIN_EXE_pocket-id-mcp`), calls tools, and verifies the effect **independently over Pocket ID's REST API** (raw reqwest, `X-API-KEY`) — e.g. `create_oidc_client` with parameters → `GET /api/oidc/clients/{id}` returns those parameters.
- Scenarios cover OIDC clients (create/update/secret-usable-via-introspection/allowed groups/delete/API definitions + client access), users (create/update/groups/custom claims/dangerous-tier delete), groups (create/update/members/custom claims/delete), admin (image byte round-trip, application configuration, API-key contract, status tools) and server behaviour (read-only gating over the wire, upstream error mapping, startup validation).
- Every test is `#[ignore]`d so `cargo test` stays hermetic; run with `cargo test --test live -- --ignored`. Env knobs: `POCKET_ID_LIVE_URL` + `POCKET_ID_LIVE_API_KEY` (existing instance), `POCKET_ID_LIVE_IMAGE`, `POCKET_ID_LIVE_PORT`.
- CI: new `live` job on every PR (Docker on `ubuntu-latest`, image pinned to `v2.13.0` — the release the vendored spec describes — so unrelated PRs don't break on upstream changes; the weekly spec-drift workflow stays the upstream tracker).
- `rmcp` dev-dependency gains the `client` + `transport-child-process` features.
- Remove `scripts/e2e-live.py` (superseded); keep `scripts/e2e-oauth.py` (OAuth + PKCE flow with cloudflared, not replaced). README gains a *Development* section.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `server-runtime`: adds a requirement that tool effects are verified live against a real Pocket ID, independently of the server's own client.

## Impact

- `tests/live/{main,common,oidc,users,groups,admin,server}.rs`, `tests/live/fixtures/logo.png`: new.
- `Cargo.toml`: rmcp dev-dependency features.
- `.github/workflows/ci.yml`: `live` job.
- `README.md`, `scripts/README.md`: docs; `scripts/e2e-live.py`: deleted.
- Findings surfaced (not fixed here): Pocket ID 2.14 secret endpoint change; `create_api_key`/`renew_api_key` cannot succeed under API-key auth (upstream 403 `api_key_auth_not_allowed`).
