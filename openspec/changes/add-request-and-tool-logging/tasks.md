## 1. Spike: span propagation

- [ ] 1.1 Add a temporary `tracing` span in the axum layer and a temporary event inside `ServerHandler::call_tool`, then issue a real `tools/call` over HTTP and confirm whether the event carries the span's fields
- [ ] 1.2 Record the outcome in `design.md` under Open Questions: either span nesting gives request/tool correlation for free, or an explicit request ID must be threaded through an axum extension or task-local
- [ ] 1.3 Remove the temporary instrumentation

## 2. Log format selection

- [ ] 2.1 Add a `LogFormat` enum (`Text`, `Json`) to `src/config.rs` and parse `POCKET_ID_MCP_LOG_FORMAT`, returning `ConfigError::Invalid` for unrecognized values
- [ ] 2.2 Default the format from `std::io::IsTerminal` on stderr when the variable is unset: terminal → `Text`, otherwise `Json`
- [ ] 2.3 Rework subscriber construction in `src/main.rs` to build both format arms via `Box<dyn Layer>` and `.boxed()`, replacing the hardcoded `.with_ansi(false)` with ANSI enabled only for `Text` on a terminal
- [ ] 2.4 Ensure format selection happens before `Config::from_env` so configuration errors are themselves emitted in the selected format, or accept and document that they are not
- [ ] 2.5 Unit-test parsing: `text`, `json`, unset-with-terminal, unset-without-terminal, and an invalid value producing a non-zero exit naming the variable

## 3. Tier lookup and parameter allowlist

- [ ] 3.1 Add a `tier_for(name: &str) -> Option<Tier>` lookup over `CATALOG` in `src/tools/mod.rs`
- [ ] 3.2 Add a `Display` or equivalent string form for `Tier` so it renders as `read`/`write`/`dangerous` in log fields
- [ ] 3.3 Add the static parameter allowlist: `user_id`, `client_id`, `group_id`, `image_type`, `api_id`, `provider_id`, `token_id`, `key_id`, `credential_id`, `user_ids`, `user_group_ids`, `oidc_client_ids`
- [ ] 3.4 Implement extraction from the raw `arguments` map: match allowlisted keys at the top level only, summarize array values rather than enumerating elements, and drop everything else
- [ ] 3.5 Unit-test extraction: identifying params kept; `token`, `key`, `name`, `ttl` and a nested `config` object dropped; `token_id` and `key_id` kept despite their prefixes; a large collection summarized
- [ ] 3.6 Add a test asserting every `CATALOG` tool name resolves to a tier

## 4. Tool call logging

- [ ] 4.1 Remove `#[tool_handler(router = self.tool_router)]` from the `ServerHandler` impl in `src/server.rs` and hand-write `call_tool`, delegating to `self.tool_router.call(...)`; leave `#[prompt_handler]` in place
- [ ] 4.2 Verify the hand-written path reproduces the macro's behavior for an unregistered tool name, and add a test covering it
- [ ] 4.3 Emit one record per call with structured fields: tool name, tier, extracted parameters, outcome, and `duration_ms` — using field syntax, never interpolated into the message
- [ ] 4.4 Include the error on failed calls via the `ApiError` display form, and confirm no response content reaches the record on either the success or error path
- [ ] 4.5 Confirm the full existing test suite still passes, particularly the tier-registration and MCP conformance tests

## 5. HTTP access logging

- [ ] 5.1 Add `tower-http = { version = "0.6", features = ["trace"] }` to `Cargo.toml` and confirm `cargo tree` shows no new crates in the dependency graph
- [ ] 5.2 Apply `TraceLayer` in `build_router` so it wraps every mode's routes, including requests rejected by auth before dispatch
- [ ] 5.3 Configure `make_span_with` to record method, path, and an `actor` field declared as `tracing::field::Empty`
- [ ] 5.4 Configure `on_response` and `on_failure` to emit status and latency
- [ ] 5.5 Record the actor in `oauth_middleware` from the validated claims' subject, replacing the discarded `Ok(_claims)` binding
- [ ] 5.6 Record a fixed actor label in `static_token_middleware` on success, never the secret itself; leave the actor unrecorded in `none` mode
- [ ] 5.7 Apply the spike's outcome: rely on span nesting for correlation, or thread an explicit request ID into the tool record

## 6. Tests

- [ ] 6.1 Integration test: an authenticated request to `/mcp` produces an access record with method, path, `200`, and a latency
- [ ] 6.2 Integration test: a request with a missing or invalid bearer token produces a `401` access record and no tool call record
- [ ] 6.3 Integration test: a token failing group admission produces a `403` access record
- [ ] 6.4 Integration test: a successful tool call produces a record carrying tool name, tier, and allowlisted parameters
- [ ] 6.5 Integration test: calling a tool whose parameters include secret-bearing values produces a record containing none of those values
- [ ] 6.6 Integration test: a tool returning credential-bearing response data produces a record containing none of that data
- [ ] 6.7 Test that JSON format emits parseable objects whose keys include the record's structured fields

## 7. Documentation

- [ ] 7.1 Add `POCKET_ID_MCP_LOG_FORMAT` to the environment variable table in `README.md`
- [ ] 7.2 Document what is logged, what is deliberately never logged, and the actor semantics of each HTTP auth mode
- [ ] 7.3 Note that stdio deployments get tool records but no access records, and that no durable sink is provided
