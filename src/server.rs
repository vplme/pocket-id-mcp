//! MCP server assembly: tier-filtered router composition and server info.

use std::sync::Arc;

use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::*;
use rmcp::{ErrorData as McpError, ServerHandler, prompt_handler, tool_handler};

use crate::client::{ApiError, BinaryResponse, PocketIdClient};
use crate::config::{Config, HttpAuthMode, Transport};

/// Image responses larger than this are written to a temp file instead of
/// being embedded as an MCP image content block.
const INLINE_IMAGE_LIMIT: usize = 2 * 1024 * 1024;

/// Tools that act on "the current user". Every upstream call authenticates
/// with the server's admin API key, so "current user" is always the key's
/// owning service account — never the MCP caller. These tools are only
/// registered where the operator plausibly IS that account (stdio, and the
/// shared-secret/loopback HTTP modes); in OAuth mode, where distinct users
/// are admitted, they would let any caller silently read or mutate the
/// service account itself.
const SELF_SERVICE_TOOLS: &[&str] = &[
    "get_current_user",
    "update_current_user",
    "update_current_user_profile_picture",
    "reset_current_user_profile_picture",
    "send_current_user_email_verification",
    "verify_current_user_email",
    "list_my_authorized_clients",
    "revoke_my_authorized_client",
    "list_my_accessible_clients",
    "list_my_audit_logs",
];

#[derive(Clone)]
pub struct PocketIdServer {
    pub(crate) config: Arc<Config>,
    pub(crate) client: Arc<PocketIdClient>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl PocketIdServer {
    pub fn new(config: Arc<Config>, client: Arc<PocketIdClient>) -> Self {
        let mut tool_router =
            Self::identity_read_tools() + Self::oidc_read_tools() + Self::admin_read_tools();
        if !config.read_only {
            tool_router = tool_router
                + Self::identity_write_tools()
                + Self::oidc_write_tools()
                + Self::admin_write_tools();
            if config.allow_dangerous {
                tool_router =
                    tool_router + Self::identity_dangerous_tools() + Self::admin_dangerous_tools();
            }
        }

        let multi_caller = matches!(
            config.http.as_ref().map(|h| &h.auth),
            Some(HttpAuthMode::OAuth(_))
        );
        if multi_caller {
            tool_router
                .map
                .retain(|name, _| !SELF_SERVICE_TOOLS.contains(&name.as_ref()));
        }

        for route in tool_router.map.values_mut() {
            let mut schema = serde_json::Value::Object((*route.attr.input_schema).clone());
            collapse_nullable_types(&mut schema);
            collapse_nullable_anyof(&mut schema);
            if let serde_json::Value::Object(mut map) = schema {
                map.insert(
                    "additionalProperties".to_string(),
                    serde_json::Value::Bool(false),
                );
                route.attr.input_schema = Arc::new(map);
            }
        }

        let mut prompt_router = Self::read_prompts();
        if !config.read_only {
            prompt_router += Self::write_prompts();
        }

        Self {
            config,
            client,
            tool_router,
            prompt_router,
        }
    }

    /// Names of all currently registered tools (used by tier tests).
    pub fn registered_tool_names(&self) -> Vec<String> {
        self.tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    /// Full definitions of all registered tools (used by schema tests).
    pub fn registered_tools(&self) -> Vec<Tool> {
        self.tool_router.list_all()
    }

    pub fn registered_prompt_names(&self) -> Vec<String> {
        self.prompt_router
            .list_all()
            .into_iter()
            .map(|p| p.name.to_string())
            .collect()
    }

    /// Render a binary API response as a tool result.
    ///
    /// SVG is returned as its markup (MCP image blocks admit only raster
    /// types, and Pocket ID logos are commonly SVG); other images within the
    /// size limit are inlined as image blocks. Anything else becomes a
    /// temp-file path on stdio — where the client shares the filesystem —
    /// and a tool-level error over HTTP, where a server-local path would be
    /// useless to the caller and the kept file would leak.
    pub(crate) fn binary_result(&self, bin: BinaryResponse) -> Result<CallToolResult, McpError> {
        use base64::Engine;
        let mut bin = bin;
        if bin.bytes.len() <= INLINE_IMAGE_LIMIT && bin.content_type == "image/svg+xml" {
            match String::from_utf8(bin.bytes) {
                Ok(svg) => return Ok(CallToolResult::success(vec![ContentBlock::text(svg)])),
                // Declared SVG but not text: fall through to the generic
                // binary handling below.
                Err(e) => bin.bytes = e.into_bytes(),
            }
        }
        if bin.bytes.len() <= INLINE_IMAGE_LIMIT
            && bin.content_type.starts_with("image/")
            && bin.content_type != "image/svg+xml"
        {
            let data = base64::engine::general_purpose::STANDARD.encode(&bin.bytes);
            return Ok(CallToolResult::success(vec![ContentBlock::image(
                data,
                bin.content_type,
            )]));
        }
        if self.config.transport() == Transport::Http {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "{} response ({} bytes) cannot be returned inline over the HTTP transport; \
                 fetch it from the Pocket ID API directly",
                bin.content_type,
                bin.bytes.len(),
            ))]));
        }
        let ext = match bin.content_type.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/svg+xml" => "svg",
            "image/webp" => "webp",
            "image/x-icon" | "image/vnd.microsoft.icon" => "ico",
            _ => "bin",
        };
        let file = tempfile::Builder::new()
            .prefix("pocket-id-image-")
            .suffix(&format!(".{ext}"))
            .tempfile()
            .map_err(|e| McpError::internal_error(format!("temp file: {e}"), None))?;
        std::fs::write(file.path(), &bin.bytes)
            .map_err(|e| McpError::internal_error(format!("temp file write: {e}"), None))?;
        let (_f, path) = file
            .keep()
            .map_err(|e| McpError::internal_error(format!("temp file keep: {e}"), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "{} response ({} bytes) written to {} — delete it when done",
            bin.content_type,
            bin.bytes.len(),
            path.display()
        ))]))
    }

    /// Map an API error to a tool-level error result (not a protocol error).
    pub(crate) fn api_error_result(e: ApiError) -> CallToolResult {
        CallToolResult::error(vec![ContentBlock::text(e.to_string())])
    }
}

/// Uniform error mapping for tools returning `Result<_, String>`.
pub(crate) fn err_str(e: ApiError) -> String {
    e.to_string()
}

/// Collapse schemars' nullable type arrays (`"type": ["boolean", "null"]`,
/// produced for `Option<T>` params) to the plain type. Optionality is already
/// expressed by absence from `required`, and clients like MCP Inspector only
/// render typed inputs (e.g. a boolean toggle) for plain `"type"` strings —
/// a type array falls back to a raw JSON text field. Applied to input schemas
/// only: MCP callers omit optional params rather than sending null, while
/// responses may legitimately contain nulls that clients validate against the
/// output schema.
fn collapse_nullable_types(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let mut removed_null_type = false;
            if let Some(serde_json::Value::Array(types)) = map.get_mut("type") {
                let before = types.len();
                types.retain(|t| t != "null");
                removed_null_type = types.len() != before;
                if types.len() == 1 {
                    let only = types.remove(0);
                    map.insert("type".to_string(), only);
                }
            }
            // An inlined `Option<Enum>` merges to `"type": ["string", "null"]`
            // with a null in the value list; dropping the null branch of the
            // type must drop it from the enum too.
            if removed_null_type {
                if let Some(serde_json::Value::Array(values)) = map.get_mut("enum") {
                    values.retain(|v| !v.is_null());
                }
            }
            for v in map.values_mut() {
                collapse_nullable_types(v);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                collapse_nullable_types(v);
            }
        }
        _ => {}
    }
}

/// Collapse schemars' nullable anyOf wrappers (`anyOf: [X, {"type": "null"}]`,
/// produced for `Option<Enum>` params) by merging X's keys into the parent
/// schema object alongside existing siblings such as `description`. Same
/// motivation as `collapse_nullable_types`: optionality is already expressed
/// by absence from `required`, and form-rendering clients only produce typed
/// inputs (e.g. an enum select) for a plain schema shape.
fn collapse_nullable_anyof(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let inner = match map.get("anyOf") {
                Some(serde_json::Value::Array(variants)) if variants.len() == 2 => {
                    let is_null = |v: &serde_json::Value| {
                        v.as_object()
                            .is_some_and(|o| o.len() == 1 && o.get("type") == Some(&"null".into()))
                    };
                    match (is_null(&variants[0]), is_null(&variants[1])) {
                        (false, true) => variants[0].as_object().cloned(),
                        (true, false) => variants[1].as_object().cloned(),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(inner) = inner {
                map.remove("anyOf");
                for (k, v) in inner {
                    map.entry(k).or_insert(v);
                }
            }
            for v in map.values_mut() {
                collapse_nullable_anyof(v);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                collapse_nullable_anyof(v);
            }
        }
        _ => {}
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for PocketIdServer {
    fn get_info(&self) -> ServerInfo {
        let mode = if self.config.read_only {
            "read-only mode: only read tools are available"
        } else if self.config.allow_dangerous {
            "all safety tiers enabled, including dangerous operations"
        } else {
            "read and write tools available; dangerous operations (user deletion, \
             passkey deletion, login-credential minting, API-key revocation) are disabled"
        };
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new(
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(format!(
            "Manage the Pocket ID instance at {} through its REST API: users, groups, \
             OIDC clients, custom claims, passkeys, branding images, audit logs, API keys, \
             and SCIM provisioning. Current safety configuration — {mode}. List endpoints \
             support page/limit/sort parameters. Image upload tools accept either a local \
             file_path or an https url.",
            self.client.base_url()
        ))
    }

    /// Dispatch a tool call, logging one record per call.
    ///
    /// Defined by hand so that every call passes through a single observable
    /// point; `#[tool_handler]` only generates this method when it is absent,
    /// so `list_tools` and `get_tool` remain macro-generated. The body is the
    /// macro's own delegation with logging around it — unknown tool names are
    /// still reported by `ToolRouter::call`, unchanged.
    ///
    /// This is the only audit trail for admin mutations made through this
    /// server: Pocket ID's audit log records sign-in events, not REST API
    /// writes. Every call is logged, reads included.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let name = request.name.to_string();
        let tier = crate::tools::tier_for(&name);
        // Read before dispatch: the router consumes the request.
        let params = crate::tools::loggable_params(request.arguments.as_ref());
        let started = std::time::Instant::now();

        // Each parameter is carried as its own `params.*` field rather than
        // one encoded string, so it stays queryable in JSON output. Field
        // names must be known at compile time and `record` only fills a field
        // the span already declares, so every allowlisted name is declared
        // here as `Empty`; the ones absent from this call are never rendered.
        let span = tracing::info_span!(
            "tool_params",
            params.user_id = tracing::field::Empty,
            params.client_id = tracing::field::Empty,
            params.group_id = tracing::field::Empty,
            params.image_type = tracing::field::Empty,
            params.api_id = tracing::field::Empty,
            params.provider_id = tracing::field::Empty,
            params.token_id = tracing::field::Empty,
            params.key_id = tracing::field::Empty,
            params.credential_id = tracing::field::Empty,
            params.secret_id = tracing::field::Empty,
            params.user_ids = tracing::field::Empty,
            params.user_group_ids = tracing::field::Empty,
            params.oidc_client_ids = tracing::field::Empty,
        );
        for (field, value) in &params {
            span.record(*field, value.as_str());
        }

        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        let result = self.tool_router.call(tcc).await;

        // Entered *after* the await, immediately around the record below,
        // rather than held across it: a synchronous span guard does not travel
        // with a future resumed on another task.
        let _entered = span.enter();

        // Field names are stable so both output formats stay queryable; the
        // tier is rendered as a string rather than Debug for the same reason.
        let tier_field = tier.map(|t| t.as_str()).unwrap_or("unknown");
        // `u64`, not `as_millis()`'s `u128`: `u128` has no `tracing::Value`
        // impl, so it falls back to `Debug` and is emitted as the *string*
        // `"6"` in JSON output rather than a number, which a collector cannot
        // range-query. Saturating rather than wrapping on the absurd overflow.
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match &result {
            Ok(response) => {
                // A tool-level failure (`isError`) is an outcome, not a
                // transport error, so it is reported separately from a
                // protocol error. Non-`Complete` variants are unreachable for
                // this server's tools but the enum is non-exhaustive.
                let outcome = match response {
                    CallToolResponse::Complete(r) if r.is_error.unwrap_or(false) => "error",
                    CallToolResponse::Complete(_) => "ok",
                    _ => "incomplete",
                };
                tracing::info!(
                    tool = %name,
                    tier = tier_field,
                    outcome,
                    duration_ms,
                    "tool call"
                );
            }
            // `ErrorData`'s message is already sanitized upstream: tool bodies
            // build it from `ApiError`, which extracts an error message without
            // echoing credentials. Response content is never logged.
            Err(e) => {
                tracing::info!(
                    tool = %name,
                    tier = tier_field,
                    outcome = "failed",
                    error = %e.message,
                    duration_ms,
                    "tool call"
                );
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn server(transport: &str) -> PocketIdServer {
        let mut vars = HashMap::from([
            (
                "POCKET_ID_URL".to_string(),
                "https://id.example.com".to_string(),
            ),
            ("POCKET_ID_API_KEY".to_string(), "k".to_string()),
            ("POCKET_ID_MCP_TRANSPORT".to_string(), transport.to_string()),
        ]);
        if transport == "http" {
            vars.insert("POCKET_ID_MCP_HTTP_AUTH".to_string(), "none".to_string());
        }
        let config = Arc::new(Config::from_vars(&vars).unwrap());
        let client = Arc::new(PocketIdClient::new("https://id.example.com", "k".into()));
        PocketIdServer::new(config, client)
    }

    fn text_of(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect()
    }

    #[test]
    fn svg_returned_as_markup_not_image_block() {
        // MCP image blocks admit only raster types; an image/svg+xml block
        // would be rejected by clients. The markup itself is the useful form.
        let result = server("stdio")
            .binary_result(BinaryResponse {
                bytes: b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec(),
                content_type: "image/svg+xml".to_string(),
            })
            .unwrap();
        assert!(!result.is_error.unwrap_or(false));
        assert!(text_of(&result).contains("<svg"), "got: {result:?}");
        assert!(
            result.content.iter().all(|c| c.as_image().is_none()),
            "svg must not be an image block"
        );
    }

    #[test]
    fn raster_image_inlined_as_image_block() {
        let result = server("stdio")
            .binary_result(BinaryResponse {
                bytes: b"\x89PNG fake".to_vec(),
                content_type: "image/png".to_string(),
            })
            .unwrap();
        let image = result
            .content
            .iter()
            .find_map(|c| c.as_image())
            .expect("an image block");
        assert_eq!(image.mime_type, "image/png");
    }

    #[test]
    fn oversized_payload_on_stdio_written_to_temp_file() {
        let result = server("stdio")
            .binary_result(BinaryResponse {
                bytes: vec![0u8; INLINE_IMAGE_LIMIT + 1],
                content_type: "image/png".to_string(),
            })
            .unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = text_of(&result);
        let path = text
            .split_whitespace()
            .find(|w| w.contains("pocket-id-image-"))
            .expect("a temp path in the message")
            .to_string();
        assert!(std::path::Path::new(&path).exists(), "file at {path}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn oversized_payload_on_http_is_an_error_without_temp_file() {
        // A server-local path is useless to a remote HTTP client, and the
        // kept file would leak on the server for every such call.
        let result = server("http")
            .binary_result(BinaryResponse {
                bytes: vec![0u8; INLINE_IMAGE_LIMIT + 1],
                content_type: "image/png".to_string(),
            })
            .unwrap();
        assert!(result.is_error.unwrap_or(false));
        let text = text_of(&result);
        assert!(text.contains("HTTP transport"), "got: {text}");
        assert!(!text.contains("pocket-id-image-"), "no path leaked: {text}");
    }
}
