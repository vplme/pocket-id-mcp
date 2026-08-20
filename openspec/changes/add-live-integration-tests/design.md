# Design: add-live-integration-tests

## Shape

- **Gherkin features + cucumber-rs** (`tests/features/*.feature`, runner/World/steps in `tests/live/`): the feature text is the readable spec (mirrors the OpenSpec scenarios), steps are domain language ("I create a confidential OIDC client …", "Pocket ID has an OIDC client …"), and a two-column data table is used only where the point is "these N fields persist". One `harness = false` target shares a single bootstrapped instance; `POCKET_ID_LIVE=1` opts in, otherwise the binary exits early so `cargo test` stays hermetic while the suite still compiles in the default CI job (no rot).
- **Typed tables**: `When … with:` cells are coerced by the tool's advertised `inputSchema` (booleans, numbers, arrays split on `, `; unknown parameters fail loudly); `Then Pocket ID's record … has:` cells are typed by the record's own JSON value.
- **World** (`world.rs`): per-scenario MCP server (`Mode::DEFAULT/READ_ONLY/DANGEROUS`), `{unique}` placeholder expansion, "that client/user/group/API definition" references, last tool error, cleanup list flushed in an `after` hook.
- **Fixture** (`common.rs`): `LiveEnv::acquire()` behind a `tokio::sync::OnceCell`. Env-supplied instance wins; otherwise `docker rm -f` + `docker run` of the pinned image on a fixed host port (APP_URL must be host-visible, so the port is chosen up front), `/healthz` wait, then `/api/signup/setup` → admin cookie (forwarded by hand: it is flagged `Secure` on plain http) → two API keys (suite key + a spare for the revocation scenario, since key creation is impossible under API-key auth). Container is left running for inspection; the next run replaces it.
- **Driving the real binary**: `rmcp` client over `TokioChildProcess` spawning `env!("CARGO_BIN_EXE_pocket-id-mcp")` with `POCKET_ID_*` env per scenario. Helpers (`common.rs`): `call` (must succeed), `call_err` (must be a tool error), `try_call` (protocol-level result, for unregistered tools), `call_json` (structured payload; falls back to JSON text).
- **Independent verification**: raw `reqwest` against Pocket ID with `X-API-KEY` — never through `PocketIdClient` — so a bug shared by tool and client cannot cancel out. A fresh `reqwest::Client` per call: pools are bound to the runtime that created them and each `#[tokio::test]` has its own.
- **Isolation**: `{unique}` per scenario, best-effort REST cleanup, up to 8 scenarios run concurrently within one container.
- **Write-only values** are proven by use, not by read-back: the client secret must authenticate to `/api/oidc/introspect` (200 vs 401), the spare API key must authenticate `GET /api/users/me` until revoked.

## Decisions

- Docker CLI over `testcontainers`: no heavy dev-deps, mirrors the existing scripts, keeps the container around for inspection.
- Pin the image to the vendored-spec release (`v2.13.0`), override via `POCKET_ID_LIVE_IMAGE`. Running `:latest` on PRs would fail unrelated changes on upstream drift (already true for 2.14).
- Pin observed upstream contracts as scenarios (API-key creation refused under API-key auth) so that, if upstream changes, the suite says so loudly and the docs/tool descriptions get revisited.
- Cucumber over plain `#[tokio::test]` (the first iteration, kept in history): ~2× the Rust for the same coverage, but the features read as the spec and the step library plateaus quickly (users/groups/oidc share the "record has:" / "no longer has" shapes). Costs accepted: `harness = false` (env opt-in instead of `#[ignore]`), one extra indirection when debugging (failure output names feature line + step fn + full record).
