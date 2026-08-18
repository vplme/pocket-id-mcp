use std::process::ExitCode;
use std::sync::Arc;

use rmcp::ServiceExt;
use rmcp::transport::stdio;

use pocket_id_mcp::client::{ApiError, NO_BODY, PocketIdClient};
use pocket_id_mcp::config::{Config, LogFormat, Transport};
use pocket_id_mcp::server::PocketIdServer;

/// Install the stderr tracing subscriber in the selected format.
///
/// The two formats are different concrete types, so they are erased behind
/// `Box<dyn Layer>` rather than duplicating the terminal `init()` call.
/// Logging goes to stderr in both transports: in stdio mode stdout carries
/// the MCP protocol and must stay clean.
fn init_logging(format: LogFormat) {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, Layer, fmt};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "pocket_id_mcp=info".into());

    let layer = match format {
        // ANSI only for a human at a terminal — the sole reason Text is chosen
        // by default — so a forced `text` in a pipe stays escape-free.
        LogFormat::Text => {
            let ansi = std::io::IsTerminal::is_terminal(&std::io::stderr());
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(ansi)
                .boxed()
        }
        // Spans are flattened into each record: rmcp dispatches handlers on
        // detached tasks, so there is no span nesting to preserve anyway.
        LogFormat::Json => fmt::layer()
            .json()
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .boxed(),
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .init();
}

#[tokio::main]
async fn main() -> ExitCode {
    // Resolved before Config so that configuration errors below are themselves
    // reported through the operator's chosen format.
    let log_format = match LogFormat::from_env() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("pocket-id-mcp: configuration error: {e}");
            return ExitCode::FAILURE;
        }
    };
    init_logging(log_format);

    let config = match Config::from_env() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("pocket-id-mcp: configuration error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let client = Arc::new(PocketIdClient::new(
        &config.pocket_id_url,
        config.api_key.clone(),
    ));

    // Startup connectivity validation: distinguish unreachable from unauthorized.
    match client
        .json::<serde_json::Value>(reqwest::Method::GET, "/api/version/current", &[], NO_BODY)
        .await
    {
        Ok(v) => {
            tracing::info!(
                version = v
                    .get("currentVersion")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown"),
                url = %config.pocket_id_url,
                "connected to Pocket ID"
            );
        }
        Err(ApiError::Api { status, .. })
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN =>
        {
            eprintln!(
                "pocket-id-mcp: API key rejected by {} (HTTP {}): check POCKET_ID_API_KEY",
                config.pocket_id_url, status
            );
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!(
                "pocket-id-mcp: cannot reach Pocket ID instance at {}: {e}",
                config.pocket_id_url
            );
            return ExitCode::FAILURE;
        }
    }

    match config.transport {
        Transport::Stdio => {
            let server = PocketIdServer::new(config.clone(), client);
            tracing::info!(
                tools = server.registered_tool_names().len(),
                read_only = config.read_only,
                allow_dangerous = config.allow_dangerous,
                "serving MCP over stdio"
            );
            let service = match server.serve(stdio()).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("pocket-id-mcp: failed to start stdio server: {e}");
                    return ExitCode::FAILURE;
                }
            };
            if let Err(e) = service.waiting().await {
                eprintln!("pocket-id-mcp: server error: {e}");
                return ExitCode::FAILURE;
            }
        }
        Transport::Http => {
            if let Err(e) = pocket_id_mcp::http::serve(config, client).await {
                eprintln!("pocket-id-mcp: HTTP server error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
