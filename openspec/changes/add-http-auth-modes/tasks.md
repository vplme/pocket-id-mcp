# Tasks: add-http-auth-modes

## 1. Configuration

- [ ] 1.1 Add `HttpAuthMode` enum (`OAuth { issuer, allowed_groups, groups_claim }`, `StaticToken { token }`, `None`) and restructure `HttpConfig` to `{ bind, public_url, auth }` in `src/config.rs`
- [ ] 1.2 Parse `POCKET_ID_MCP_HTTP_AUTH` (default `oauth`; unknown value → `ConfigError::Invalid` naming accepted values) and `POCKET_ID_MCP_HTTP_TOKEN`
- [ ] 1.3 Implement the mode/variable compatibility matrix: reject OAuth-only vars in `token`/`none`, reject `HTTP_TOKEN` outside `token` mode, require non-empty `HTTP_TOKEN` in `token` mode
- [ ] 1.4 Make `POCKET_ID_MCP_PUBLIC_URL` optional outside `oauth` mode, defaulting to `http://localhost:<bind port>`
- [ ] 1.5 Implement the loopback guard for `none` mode (`127.0.0.0/8`, `::1`, `localhost`) with `POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK` override
- [ ] 1.6 Update existing config tests for the restructured `HttpConfig`; add unit tests covering the full matrix (default-oauth unchanged, each rejection, public-url defaulting, loopback guard accept/refuse/override)

## 2. HTTP transport

- [ ] 2.1 Rework `HttpState`/`serve()` in `src/http/mod.rs` so `Authenticator` construction, the startup issuer/JWKS probe, and the metadata routes exist only in `oauth` mode
- [ ] 2.2 Add static-token middleware: bearer extraction as today, constant-time compare via `subtle::ConstantTimeEq` (add `subtle` to `Cargo.toml`), `401` + `WWW-Authenticate: Bearer` without `resource_metadata` on failure
- [ ] 2.3 Wire mode-based middleware selection in `build_router()` (`oauth` → existing middleware, `token` → static-token middleware, `none` → no auth layer), keeping rmcp `allowed_hosts` active in all modes
- [ ] 2.4 Log the active auth mode at startup (`auth_mode = ...`), keeping the existing oauth log line for that mode

## 3. Integration tests

- [ ] 3.1 `token` mode: matching token → 200 MCP response; wrong/missing token → 401 without `resource_metadata`; metadata route absent (404)
- [ ] 3.2 `none` mode: request without `Authorization` header is processed; metadata route absent; spoofed `Host` header still rejected (DNS-rebinding guard)
- [ ] 3.3 `oauth` mode regression: existing HTTP integration tests still pass unchanged

## 4. Documentation

- [ ] 4.1 Update README: `POCKET_ID_MCP_HTTP_AUTH` in the configuration table with the new variables, a local-HTTP quick-start example, and an explicit warning about `none` mode + the non-loopback override
- [ ] 4.2 Verify `cargo fmt --check`, `cargo clippy`, and `cargo test` all pass
