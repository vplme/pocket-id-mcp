## Why

Once started, the server goes silent: there are no logs for HTTP requests, authentication outcomes, or tool calls. That matters more here than in a typical service, because Pocket ID's own audit log records **sign-in events** (its DTO carries `actorUsername`, `ipAddress`, `country`, `city`, `device`) and not admin REST API mutations. When an MCP client deletes a user, revokes an API key, or rewrites application configuration through this server, **no record of it exists anywhere** — not upstream, not here. Logging the tool call is therefore the only possible audit trail for admin mutations, not a duplicate of one.

A second gap: in HTTP mode the server holds an admin API key and is reachable over the network, yet repeated `401`/`403` probes — the signature of someone attacking it — are equally invisible.

## What Changes

- **Access logging (HTTP transport):** a per-request span via `tower-http`'s `TraceLayer` recording method, path, status, and latency — including requests rejected by the auth middleware before they reach a tool.
- **Actor attribution:** the OAuth claims validated in `oauth_middleware` are currently discarded (`Ok(_claims) => next.run(request).await`). The token subject is recorded onto the request span so log lines carry *who*. In `token` mode the actor is the shared secret's fixed label; in `none` mode there is no actor.
- **Tool call logging (both transports):** every `tools/call` is logged — reads included — with tool name, safety tier, outcome, and duration. This requires replacing the macro-generated `#[tool_handler]` with a hand-written `call_tool` that wraps `self.tool_router.call(...)`, giving a single chokepoint instead of instrumenting 84 tool functions.
- **Argument logging via allowlist:** identifying request parameters (`user_id`, `client_id`, `group_id`, `image_type`, `api_id`, `provider_id`, `token_id`, `key_id`, `credential_id`, and the `*_ids` collection params) are logged so a line says *what the call acted on*. Everything else is dropped. A denylist was rejected: `token_id` (safe identifier) and `token` (a real bearer token, `src/tools/oidc.rs:68`) differ only by suffix, so blocklisting degrades into whack-a-mole, whereas an allowlist fails closed.
- **Response bodies are never logged.** Read-tier tools are where the secrets live — `get_all_application_configuration` returns LDAP and SMTP credentials, `list_api_keys` returns key metadata, `introspect_token` returns token contents. Logging request-side identifiers only keeps the leak surface closed.
- **Selectable log format:** `POCKET_ID_MCP_LOG_FORMAT=text|json`, defaulting by TTY detection — human-readable with ANSI when stderr is a terminal, JSON when it is a pipe (Docker, systemd, k8s). Replaces the currently hardcoded `.with_ansi(false)`.
- **Out of scope:** the stdio transport gets tool logging (it is transport-independent) but no access logging, since stdio has no HTTP requests. A durable file sink for stdio deployments is deliberately deferred — it would introduce the first local state into a binary whose selling point is "single static binary, no config file."

## Capabilities

### New Capabilities
- `observability`: what the server logs and — equally binding — what it must never log; log formats and their selection; actor attribution per auth mode.

### Modified Capabilities
- `server-runtime`: the "Environment-based configuration" requirement gains `POCKET_ID_MCP_LOG_FORMAT`. The tool-dispatch path changes from macro-generated to hand-written, which the existing "Safety-tier tool registration" requirement must continue to hold across.

## Impact

- **Code:** `src/main.rs` (subscriber construction, format selection, TTY detection), `src/http/mod.rs` (`TraceLayer`, actor recording in both auth middlewares), `src/server.rs` (hand-written `call_tool` replacing `#[tool_handler]`), `src/tools/mod.rs` (tool-name → `Tier` lookup, currently only in the static `CATALOG`; parameter allowlist).
- **Dependencies:** `tower-http` with the `trace` feature. Already present in `Cargo.lock` at 0.6.11 via reqwest, but compiled without `trace` — so this adds a `Cargo.toml` entry and a feature-unification rebuild, and **zero new crates** to the tree.
- **Behavior:** default log volume rises substantially, since every read is logged. Existing `RUST_LOG` handling and the `pocket_id_mcp=info` default filter are unchanged.
- **Risk / open question:** whether the `TraceLayer` request span is still current when `call_tool` executes depends on whether `StreamableHttpService` dispatches on the same task or a spawned one. If spawned, request/tool correlation needs an explicit request ID rather than coming free from span nesting. This is verified by a spike before the correlation work.
