# Tasks: add-live-integration-tests

## 1. Harness

- [x] 1.1 Dev-dependencies: `rmcp` `client` + `transport-child-process` features; `cucumber` with `macros`; `[[test]] live` with `harness = false`
- [x] 1.2 `tests/live/common.rs`: Docker/env fixture, bootstrap via `/api/signup/setup`, REST helpers, MCP client over the real binary, naming/cleanup helpers
- [x] 1.3 `tests/live/fixtures/logo.png` (8×8 PNG for the image round-trip)
- [x] 1.4 `tests/live/{main,world}.rs`: cucumber runner (env opt-in, `@needs-bootstrap` filter, after-hook cleanup), World with schema-typed tables and "that X" references

## 2. Scenarios

- [x] 2.1 `oidc_clients.feature` + `steps/oidc.rs`: create visible via REST, update persists, secret usable via introspection + rotation, allowed groups, delete → 404, API definition + client access
- [x] 2.2 `users.feature` + `steps/users.rs`: create, update, group membership, custom claims, dangerous-tier delete
- [x] 2.3 `groups.feature` + `steps/groups.rs`: create, update, members (set + clear), custom claims, delete
- [x] 2.4 `admin.feature` + `steps/admin.rs`: API-key contract (creation refused, revocation works), image byte round-trip, application configuration, status tools
- [x] 2.5 `server.feature` + `steps/server.rs`: read-only gating over the wire, upstream error mapping, startup validation (bad key, unreachable)

## 3. CI and docs

- [x] 3.1 `ci.yml` `live` job (Docker, pinned image)
- [x] 3.2 README *Development* section; `scripts/e2e-live.py` removed, `scripts/README.md` updated

## 4. Verification

- [x] 4.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (live binary exits with its opt-in notice)
- [x] 4.2 `POCKET_ID_LIVE=1 cargo test --test live` green against `v2.13.0` (25 scenarios / 120 steps)
- [x] 4.3 Negative check: a deliberately broken tool makes the corresponding live test fail with a clear message
