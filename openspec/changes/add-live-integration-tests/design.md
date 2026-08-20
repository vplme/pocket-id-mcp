# Design: add-live-integration-tests

## Shape

- **One test target** (`tests/live/main.rs` + modules) so the whole suite shares a single bootstrapped instance; `#[ignore]` per test keeps `cargo test` hermetic while the suite still compiles in the default CI job (no rot).
- **Fixture** (`common.rs`): `LiveEnv::acquire()` behind a `tokio::sync::OnceCell`. Env-supplied instance wins; otherwise `docker rm -f` + `docker run` of the pinned image on a fixed host port (APP_URL must be host-visible, so the port is chosen up front), `/healthz` wait, then `/api/signup/setup` → admin cookie (forwarded by hand: it is flagged `Secure` on plain http) → two API keys (suite key + a spare for the revocation scenario, since key creation is impossible under API-key auth). Container is left running for inspection; the next run replaces it.
- **Driving the real binary**: `rmcp` client over `TokioChildProcess` spawning `env!("CARGO_BIN_EXE_pocket-id-mcp")` with `POCKET_ID_*` env per scenario (`Mode::DEFAULT / READ_ONLY / DANGEROUS`). Helpers: `call` (must succeed), `call_err` (must be a tool error), `try_call` (protocol-level result, for unregistered tools), `call_json` (structured payload; falls back to JSON text).
- **Independent verification**: raw `reqwest` against Pocket ID with `X-API-KEY` — never through `PocketIdClient` — so a bug shared by tool and client cannot cancel out. A fresh `reqwest::Client` per call: pools are bound to the runtime that created them and each `#[tokio::test]` has its own.
- **Isolation**: unique names per test (`unique(prefix)`), best-effort REST cleanup, tests run in parallel within one container.
- **Write-only values** are proven by use, not by read-back: the client secret must authenticate to `/api/oidc/introspect` (200 vs 401), the spare API key must authenticate `GET /api/users/me` until revoked.

## Decisions

- Docker CLI over `testcontainers`: no heavy dev-deps, mirrors the existing scripts, keeps the container around for inspection.
- Pin the image to the vendored-spec release (`v2.13.0`), override via `POCKET_ID_LIVE_IMAGE`. Running `:latest` on PRs would fail unrelated changes on upstream drift (already true for 2.14).
- Pin observed upstream contracts as tests (API-key creation refused under API-key auth) so that, if upstream changes, the suite says so loudly and the docs/tool descriptions get revisited.
