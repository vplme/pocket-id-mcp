# Live end-to-end test drivers

Manual verification scripts that exercise `pocket-id-mcp` against a real, throwaway
Pocket ID instance. The script bootstraps everything itself — fresh container, first-admin
account via the one-time `/api/signup/setup` call (no passkey needed), API key — and tears
nothing down on failure so you can inspect state.

> The stdio-mode pass (users, groups, OIDC clients, images, …) now lives in the Rust live
> suite under `tests/live/` and runs in CI: `cargo test --test live -- --ignored`. See the
> *Development* section of the top-level README.

Requirements: Docker, Python 3 with `requests`, a debug build (`cargo build`), and
`cloudflared` (quick tunnel; no account required).

- **`e2e-oauth.py`** — HTTP-mode pass: full OAuth 2.1 authorization-code + PKCE flow as
  an MCP client would run it (RFC 9728 metadata → OIDC discovery → consent → token),
  RFC 8707 audience-bound tokens, MCP tool calls over Streamable HTTP, wrong-audience
  rejection — for both a pre-registered public client and CIMD self-registration.
  The CIMD metadata document is hosted through a cloudflared quick tunnel because
  Pocket ID's CIMD fetcher refuses private addresses (SSRF protection), so a genuinely
  public https URL is required.

Run from the repository root:

```sh
cargo build
python3 scripts/e2e-oauth.py
```
