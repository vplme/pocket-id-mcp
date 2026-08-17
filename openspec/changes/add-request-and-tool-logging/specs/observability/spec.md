## ADDED Requirements

### Requirement: Tool call logging
The server SHALL emit one log record for every `tools/call` request in every transport, including read-tier calls. Each record SHALL carry the tool name, its safety tier (`read`, `write`, or `dangerous`), the call outcome (success or failure), and the call duration in milliseconds. Failed calls SHALL additionally carry the error. This record is the only audit trail that exists for admin mutations performed through the server, since the Pocket ID audit log records sign-in events and not REST API mutations.

#### Scenario: Successful read-tier call logged
- **WHEN** an MCP client successfully calls `list_users`
- **THEN** a log record is emitted naming the tool, tier `read`, a success outcome, and a duration

#### Scenario: Dangerous-tier mutation logged
- **WHEN** an MCP client successfully calls `delete_user`
- **THEN** a log record is emitted naming the tool, tier `dangerous`, the target user ID, and a success outcome

#### Scenario: Failed call logged with error
- **WHEN** a tool call returns an API error or an error result
- **THEN** a log record is emitted with a failure outcome and the error, and the client still receives its normal error result

#### Scenario: Unknown tool
- **WHEN** a client calls a tool name that is not registered
- **THEN** the dispatch behaves exactly as before this change (the client receives a method-not-found error) and the attempt is logged

### Requirement: Request parameter logging by allowlist
Tool call records SHALL include request parameters whose names appear in a static allowlist of identifying parameters, so that a record states what the call acted upon. Parameters not on the allowlist SHALL be omitted entirely. The allowlist SHALL contain identifier and identifier-collection parameters only — at minimum `user_id`, `client_id`, `group_id`, `image_type`, `api_id`, `provider_id`, `token_id`, `key_id`, `credential_id`, `user_ids`, `user_group_ids`, and `oidc_client_ids`. A parameter name absent from the allowlist SHALL be dropped rather than logged, so that newly added tools fail closed.

Each logged parameter SHALL be emitted as its own named field, namespaced under `params.` — `params.user_id`, `params.group_id`, and so on — rather than collapsed into a single field holding an encoded string. The namespace follows the dotted convention used by OpenTelemetry semantic conventions and Elastic Common Schema, keeps parameters visually grouped in text output, and prevents a tool parameter from ever colliding with a server-side field name such as `tool` or `outcome`. A parameter that is absent from a call SHALL NOT be rendered at all, so a call with no allowlisted parameters emits no parameter fields rather than an empty one.

#### Scenario: Identifying parameter logged
- **WHEN** `set_user_groups` is called with a `user_id` and a list of `user_group_ids`
- **THEN** the log record carries the user ID and the group ID collection as separate `params.user_id` and `params.user_group_ids` fields

#### Scenario: Parameters queryable in structured output
- **WHEN** a tool call is logged in JSON format with an allowlisted parameter present
- **THEN** that parameter appears as a discrete key named `params.<name>` whose value is the parameter value, not as text embedded in another field's string

#### Scenario: Absent parameters emit no fields
- **WHEN** a tool is called with no allowlisted parameters, such as `list_users`
- **THEN** the record carries no `params.` field at all, rather than an empty one

#### Scenario: Secret-bearing parameter omitted
- **WHEN** `introspect_token` is called with a `token` parameter holding a bearer token
- **THEN** the log record names the tool but contains no part of the token value

#### Scenario: Non-allowlisted parameter dropped
- **WHEN** a tool is called with parameters that are not on the allowlist (for example `name`, `ttl`, or a nested `config` object)
- **THEN** those values do not appear in any log record

#### Scenario: Collection parameters bounded
- **WHEN** a call passes an identifier collection large enough to make a log line unwieldy
- **THEN** the record summarizes the collection rather than emitting every element

### Requirement: Response bodies are never logged
The server SHALL NOT write any part of a Pocket ID API response body, or any tool result content, to its logs at any log level. Read-tier tools return credential material — `get_all_application_configuration` returns LDAP and SMTP settings, `list_api_keys` returns key records, `create_oidc_client_secret` returns a client secret, and one-time access tools return usable tokens — so the prohibition covers success responses as well as errors. Error records SHALL carry only the already-sanitized `ApiError` display form, which extracts a message without echoing credentials.

#### Scenario: Configuration read does not leak credentials
- **WHEN** `get_all_application_configuration` succeeds and returns LDAP bind credentials
- **THEN** no log record contains any field of that response

#### Scenario: Secret-returning mutation does not leak
- **WHEN** `create_oidc_client_secret` succeeds
- **THEN** the log record confirms the call succeeded and contains no secret value

### Requirement: HTTP access logging
In HTTP transport the server SHALL emit one log record per HTTP request carrying the request method, path, response status, and latency. Records SHALL be emitted for requests rejected by authentication before reaching a tool, so that repeated `401` and `403` responses are visible. Access logging SHALL apply in all three HTTP authentication modes.

#### Scenario: Successful MCP request logged
- **WHEN** an authenticated client posts an MCP request to `/mcp`
- **THEN** a record is emitted with the method, path, a `200` status, and a latency

#### Scenario: Rejected request logged
- **WHEN** a request arrives with a missing or invalid bearer token and is rejected
- **THEN** a record is emitted showing the `401` status, and no tool call record is emitted

#### Scenario: Forbidden request logged
- **WHEN** a validated token fails group admission and is rejected with `403`
- **THEN** a record is emitted showing the `403` status

### Requirement: Actor attribution in HTTP mode
HTTP access records SHALL identify the caller according to the active authentication mode: in `oauth` mode the subject claim of the validated token; in `token` mode a fixed label denoting the shared secret, since all callers are indistinguishable; in `none` mode no actor. The server SHALL record the actor only after the token has been validated, and SHALL NOT log token values, raw credentials, or the shared secret itself.

#### Scenario: OAuth subject attributed
- **WHEN** a request is admitted in `oauth` mode
- **THEN** its access record carries the token's subject claim as the actor

#### Scenario: Static token mode attributed honestly
- **WHEN** a request is admitted in `token` mode
- **THEN** its access record carries a fixed actor label and does not contain the shared secret

#### Scenario: Unauthenticated mode has no actor
- **WHEN** a request is served in `none` mode
- **THEN** its access record carries no actor

### Requirement: Selectable log output format
The server SHALL support a human-readable text format and a machine-parseable JSON format, selected by `POCKET_ID_MCP_LOG_FORMAT` (`text` or `json`). When the variable is unset the server SHALL choose by inspecting whether its log stream is a terminal: text with ANSI styling when attached to a terminal, JSON without styling otherwise, so that container and service deployments emit structured output with no configuration. Log records SHALL be emitted with named fields rather than values interpolated into the message text, so that both formats carry the same queryable structure.

#### Scenario: JSON selected explicitly
- **WHEN** the server starts with `POCKET_ID_MCP_LOG_FORMAT=json`
- **THEN** log records are emitted as JSON objects with the record fields as JSON keys

#### Scenario: Text selected explicitly
- **WHEN** the server starts with `POCKET_ID_MCP_LOG_FORMAT=text`
- **THEN** log records are emitted in human-readable form even when the log stream is not a terminal

#### Scenario: Format defaulted for a container
- **WHEN** the server starts with `POCKET_ID_MCP_LOG_FORMAT` unset and its log stream is not a terminal
- **THEN** records are emitted as JSON without ANSI escape sequences

#### Scenario: Invalid format value
- **WHEN** `POCKET_ID_MCP_LOG_FORMAT` is set to an unrecognized value
- **THEN** the server exits non-zero before serving with a message naming the variable and the accepted values

### Requirement: Logging respects existing filter configuration
Log records SHALL be emitted through the existing `tracing` subscriber and remain subject to `RUST_LOG`, preserving the current default filter of `pocket_id_mcp=info`. Logging SHALL be written to the standard error stream, leaving standard output free for the stdio transport's MCP protocol traffic.

#### Scenario: Default filter shows tool calls
- **WHEN** the server runs with `RUST_LOG` unset
- **THEN** tool call records and HTTP access records are emitted under the default filter

#### Scenario: Protocol stream is not polluted
- **WHEN** the server runs in stdio transport and logs a tool call
- **THEN** the record is written to standard error and standard output carries only MCP protocol messages
