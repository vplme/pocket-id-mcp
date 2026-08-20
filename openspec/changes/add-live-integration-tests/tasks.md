# Tasks: add-live-integration-tests

## 1. Harness

- [x] 1.1 `rmcp` dev-dependency: `client` + `transport-child-process` features
- [x] 1.2 `tests/live/common.rs`: Docker/env fixture, bootstrap via `/api/signup/setup`, REST helpers, MCP client over the real binary, naming/cleanup helpers
- [x] 1.3 `tests/live/fixtures/logo.png` (8×8 PNG for the image round-trip)

## 2. Scenarios

- [x] 2.1 `oidc.rs`: create visible via REST, update persists, secret usable via introspection + rotation, allowed groups, delete → 404, API definition + client access
- [x] 2.2 `users.rs`: create, update, set groups, custom claims, dangerous-tier delete
- [x] 2.3 `groups.rs`: create, update, members (set + clear), custom claims, delete
- [x] 2.4 `admin.rs`: API-key contract (creation refused, revocation works), image byte round-trip, application configuration, status tools
- [x] 2.5 `server.rs`: read-only gating over the wire, upstream error mapping, startup validation (bad key, unreachable)

## 3. CI and docs

- [x] 3.1 `ci.yml` `live` job (Docker, pinned image)
- [x] 3.2 README *Development* section; `scripts/e2e-live.py` removed, `scripts/README.md` updated

## 4. Verification

- [x] 4.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (live tests reported as ignored)
- [x] 4.2 `cargo test --test live -- --ignored` green against `v2.13.0`
- [x] 4.3 Negative check: a deliberately broken tool makes the corresponding live test fail with a clear message
