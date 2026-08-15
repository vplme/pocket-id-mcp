//! MCP server assembly: tier-filtered router composition and server info.

use std::sync::Arc;

use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::*;
use rmcp::{ErrorData as McpError, ServerHandler, prompt_handler, tool_handler};

use crate::client::{ApiError, BinaryResponse, PocketIdClient};
use crate::config::Config;

/// Image responses larger than this are written to a temp file instead of
/// being embedded as an MCP image content block.
const INLINE_IMAGE_LIMIT: usize = 2 * 1024 * 1024;

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

    /// Render a binary API response as a tool result: inline MCP image block,
    /// or a temp-file path for oversized payloads.
    pub(crate) fn binary_result(&self, bin: BinaryResponse) -> Result<CallToolResult, McpError> {
        use base64::Engine;
        if bin.bytes.len() <= INLINE_IMAGE_LIMIT && bin.content_type.starts_with("image/") {
            let data = base64::engine::general_purpose::STANDARD.encode(&bin.bytes);
            return Ok(CallToolResult::success(vec![ContentBlock::image(
                data,
                bin.content_type,
            )]));
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
            "{} response ({} bytes) written to {}",
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
}
