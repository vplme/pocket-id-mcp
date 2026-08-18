## 1. Spike: span propagation

- [x] 1.1 Determine whether the axum request span reaches `call_tool` — answered from `rmcp` 3.1.2 source instead of temporary instrumentation, since the dispatch path is unambiguous there: all handlers run on a detached `tokio::spawn`, and the session worker outlives the request that spawned it
- [x] 1.2 Record the outcome in `design.md` under Open Questions: span nesting cannot correlate, and the axum-extension fallback is also unavailable; ship the two layers uncorrelated
- [x] 1.3 Remove the temporary instrumentation — not applicable, none was added

## 2. Log format selection

- [x] 2.1 Add a `LogFormat` enum (`Text`, `Json`) to `src/config.rs` and parse `POCKET_ID_MCP_LOG_FORMAT`, returning `ConfigError::Invalid` for unrecognized values
- [x] 2.2 Default the format from `std::io::IsTerminal` on stderr when the variable is unset: terminal → `Text`, otherwise `Json`
- [x] 2.3 Rework subscriber construction in `src/main.rs` to build both format arms via `Box<dyn Layer>` and `.boxed()`, replacing the hardcoded `.with_ansi(false)` with ANSI enabled only for `Text` on a terminal
- [x] 2.4 Ensure format selection happens before `Config::from_env` so configuration errors are themselves emitted in the selected format — `LogFormat::from_env` runs first and `init_logging` is installed before `Config::from_env`
- [x] 2.5 Unit-test parsing: `text`, `json`, unset-with-terminal, unset-without-terminal, and an invalid value producing a non-zero exit naming the variable — `is_terminal` is injected as a parameter so the default is testable without a pty

## 3. Tier lookup and parameter allowlist

- [x] 3.1 Add a `tier_for(name: &str) -> Option<Tier>` lookup over `CATALOG` in `src/tools/mod.rs`
- [x] 3.2 Add a `Display` or equivalent string form for `Tier` so it renders as `read`/`write`/`dangerous` in log fields
- [x] 3.3 Add the static parameter allowlist: `user_id`, `client_id`, `group_id`, `image_type`, `api_id`, `provider_id`, `token_id`, `key_id`, `credential_id`, `user_ids`, `user_group_ids`, `oidc_client_ids`
- [x] 3.4 Implement extraction from the raw `arguments` map: match allowlisted keys at the top level only, summarize array values rather than enumerating elements, and drop everything else — iterates the allowlist rather than the arguments, so an unvetted key cannot reach the log by any path
- [x] 3.5 Unit-test extraction: identifying params kept; `token`, `key`, `name`, `ttl` and a nested `config` object dropped; `token_id` and `key_id` kept despite their prefixes; a large collection summarized
- [x] 3.6 Add a test asserting every `CATALOG` tool name resolves to a tier

## 4. Tool call logging

- [x] 4.1 Hand-write `call_tool` in the `ServerHandler` impl in `src/server.rs`, delegating to `self.tool_router.call(...)` — `#[tool_handler]` is *kept*, since it only generates `call_tool` when absent (`rmcp-macros` `has_method` check), so `list_tools` and `get_tool` stay macro-generated
- [x] 4.2 Verify the hand-written path reproduces the macro's behavior for an unregistered tool name — the body is the macro's own delegation verbatim, so unknown names are still reported by `ToolRouter::call`; a test over the real HTTP dispatch path is added as 6.8
- [x] 4.3 Emit one record per call with structured fields: tool name, tier, extracted parameters, outcome, and `duration_ms` — using field syntax, never interpolated into the message
- [x] 4.4 Include the error on failed calls via the `ApiError` display form, and confirm no response content reaches the record on either the success or error path — `is_error` results log `outcome=error` without content; protocol errors log the already-sanitized `ErrorData::message`
- [x] 4.5 Confirm the full existing test suite still passes, particularly the tier-registration and MCP conformance tests — 73 tests green

## 5. HTTP access logging

- [x] 5.1 Add `tower-http = { version = "0.6", features = ["trace"] }` to `Cargo.toml` and confirm no new crates enter the dependency graph — verified, zero added
- [x] 5.2 Apply `TraceLayer` in `build_router` so it wraps every mode's routes, including requests rejected by auth before dispatch — applied outermost via `.layer()`
- [x] 5.3 Configure `make_span_with` to record method, path, and an `actor` field declared as `tracing::field::Empty`
- [x] 5.4 Configure `on_response` to emit status and latency — hand-written rather than `DefaultOnResponse`, which emits at DEBUG under the `tower_http` target and would have been dropped entirely by the default `pocket_id_mcp=info` filter (caught by the integration tests, which assert under that exact filter)
- [x] 5.5 Record the actor in `oauth_middleware` from the validated claims' subject, replacing the discarded `Ok(_claims)` binding
- [x] 5.6 Record a fixed actor label in `static_token_middleware` on success, never the secret itself; leave the actor unrecorded in `none` mode
- [x] 5.7 Apply the spike's outcome: correlation is not achievable at this rmcp version, so document in `README.md` that access and tool records are independent and joined by timestamp and actor

## 6. Tests

- [x] 6.1 Integration test: an authenticated request to `/mcp` produces an access record with method, path, `200`, and a latency
- [x] 6.2 Integration test: a request with a missing or invalid bearer token produces a `401` access record and no tool call record
- [x] 6.3 Integration test: a token failing group admission produces a `403` access record — asserted in `tests/http_auth.rs`, which already stands up the OAuth issuer and JWKS fixture
- [x] 6.4 Integration test: a successful tool call produces a record carrying tool name, tier, and allowlisted parameters
- [x] 6.5 Integration test: calling a tool whose parameters include secret-bearing values produces a record containing none of those values
- [x] 6.6 Integration test: a tool returning credential-bearing response data produces a record containing none of that data — wiremock returns LDAP/SMTP secrets, the test asserts the client received them and the log did not
- [x] 6.7 Test that JSON format emits parseable objects whose keys include the record's structured fields
- [x] 6.8 Integration test: an unregistered tool name is still reported to the client and is logged with `tier=unknown`
- [x] 6.9 Assert a `403` group-admission rejection produces an access record in `tests/http_auth.rs`, which already has the OAuth fixture
- [x] 6.10 Integration test: an admitted OAuth caller is attributed by its subject claim, with the bearer token absent from the log

## 7. Per-parameter log fields

Follow-up from running the merged build: `params` was a single field holding an encoded string, which is unqueryable in JSON and emitted `params=""` on calls with nothing allowlisted.

- [x] 7.1 Measure how `tracing` renders the candidate encodings before choosing — a `serde_json::Value` field is stringified *and escaped* in JSON mode, so it cannot give real nesting
- [x] 7.2 Change `LOGGED_PARAMS` to pair each argument name with its `params.<name>` log field, keeping the two together so they cannot drift
- [x] 7.3 Change `loggable_params` to return `(field name, value)` pairs instead of a joined string
- [x] 7.4 Carry the pairs on a `tool_params` span declaring every allowlisted name as `field::Empty` — events cannot be `record`ed, so a span is the only way to attach a dynamically-selected set of named fields — and drop the `params` field from both event sites
- [x] 7.5 Update the unit tests to the pair-returning shape, and add one asserting every allowlist field name matches `params.<argument name>`
- [x] 7.6 Integration test: an allowlisted parameter appears as its own `params.<name>` field, not as text inside another field
- [x] 7.7 Integration test: a call with no allowlisted parameters emits no `params.` field at all
- [x] 7.8 Extend the JSON test to assert parameters are discrete keys on the span and that unrecorded slots are absent
- [x] 7.9 Tighten the `observability` spec, which said only that parameters "SHALL be included" — the looseness that allowed the collapsed encoding

## 8. Review fixes

- [x] 8.1 Emit `duration_ms` as `u64` rather than `u128` on both the tool record and the access record — `u128` has no `tracing::Value` impl and fell back to `Debug`, rendering as the JSON *string* `"17"` next to a numeric `status`, so latency alerts could never match. Verified by reintroducing the fault and watching the strengthened test fail
- [x] 8.2 Bound the length of individual logged values, not just collection element counts: allowlisted names are identifiers by contract but not by validation, so an oversized value would otherwise be written verbatim. Truncates on a char boundary — a naive cut panics on multi-byte UTF-8 — and marks the value as truncated
- [x] 8.3 Strengthen the JSON test to assert `is_number()` on `duration_ms` and `status` for both record types; the previous assertion checked only presence, which is why it passed against the defect
- [x] 8.4 Add unit tests for oversized-value bounding and char-boundary safety
- [x] 8.5 Record both rules in the `observability` spec — neither had coverage, which is how the defect passed review-by-spec

## 9. Documentation

- [x] 9.1 Add `POCKET_ID_MCP_LOG_FORMAT` to the environment variable table in `README.md`
- [x] 9.2 Document what is logged, what is deliberately never logged, and the actor semantics of each HTTP auth mode — sample records were copied from real captured output rather than written by hand
- [x] 9.3 Note that stdio deployments get tool records but no access records, and that no durable sink is provided
- [x] 9.4 Update the sample records and document the `params.` field convention after the per-parameter change
- [x] 9.5 Note that a tool record trails its access record rather than nesting inside it, since the HTTP response completes while the handler runs on a detached task — relevant to joining the two layers by timestamp
