# Live end-to-end test drivers

Manual verification scripts that exercise `pocket-id-mcp` against a real, throwaway
Pocket ID instance. Both bootstrap everything themselves — fresh container, first-admin
account via the one-time `/api/signup/setup` call (no passkey needed), API key — and
tear nothing down on failure so you can inspect state.

Requirements: Docker, Python 3 with `requests`, a debug build (`cargo build`).
`e2e-oauth.py` additionally needs `cloudflared` (quick tunnel; no account required).

- **`e2e-live.py`** — stdio-mode pass: user/group CRUD, custom claims, OIDC client with
  shown-once secret and group restriction, byte-identical application-image round-trip,
  audit logs, versions, dangerous-tier deletion.
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
python3 scripts/e2e-live.py
python3 scripts/e2e-oauth.py
```

## Client-facing conformance

- **`inspector-verify.sh`** — drives a running server with the official
  [MCP Inspector](https://modelcontextprotocol.io/docs/2026-07-28/tools/inspector/cli)
  CLI and asserts `tools/list` returns a usable catalogue. It runs the check twice,
  once per protocol era: `legacy` (session id from `initialize`, reused on every
  later request) and `modern` (2026-07-28 — sessionless, with an `Mcp-Method`
  header and protocol metadata per request). Those are different code paths, so a
  server can be healthy on one and broken on the other.

  Unlike the `e2e-*.py` drivers this needs no Pocket ID instance: tool definitions
  come from the compiled catalogue, so `stub-pocket-id.py` is enough to satisfy the
  startup connectivity probe. CI runs it on every push; `tests/tools_list_wire.rs`
  covers the same two eras in-process, without Node.

  ```sh
  python3 scripts/stub-pocket-id.py --port 8899 &
  POCKET_ID_URL=http://127.0.0.1:8899 POCKET_ID_API_KEY=k \
  POCKET_ID_MCP_TRANSPORT=http POCKET_ID_MCP_HTTP_AUTH=none \
  POCKET_ID_MCP_HTTP_BIND=127.0.0.1:8757 ./target/debug/pocket-id-mcp &
  scripts/inspector-verify.sh http://127.0.0.1:8757/mcp
  ```

  Point it at any URL to check a deployed server. `REQUIRE_TOOL` changes the
  sentinel tool that must be present (default `list_users`), and
  `INSPECTOR_TIMEOUT_MS` the connect timeout — the default is 20s rather than the
  Inspector's own 60s, so a hang fails the build promptly instead of stalling it.
