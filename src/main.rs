use std::process::ExitCode;
use std::sync::Arc;

use rmcp::ServiceExt;
use rmcp::transport::stdio;

use pocket_id_mcp::client::{ApiError, NO_BODY, PocketIdClient};
use pocket_id_mcp::config::{Config, Transport};
use pocket_id_mcp::server::PocketIdServer;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pocket_id_mcp=info".into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

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
                version = version_from_payload(&v).unwrap_or("unknown"),
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

/// Pull the version string out of `GET /api/version/current`.
///
/// The endpoint is typed as `map[string]string` in the Pocket ID swagger, so the
/// key is not contractual: v2.13 answers `{"currentVersion":"2.13.0"}`. Accept the
/// known spellings and otherwise fall back to any single string value.
fn version_from_payload(v: &serde_json::Value) -> Option<&str> {
    for key in ["currentVersion", "version", "current_version"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            return Some(s);
        }
    }
    match v.as_object() {
        Some(map) if map.len() == 1 => map.values().next().and_then(|x| x.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::version_from_payload;
    use serde_json::json;

    #[test]
    fn reads_pocket_id_current_version_key() {
        assert_eq!(
            version_from_payload(&json!({"currentVersion": "2.13.0"})),
            Some("2.13.0")
        );
    }

    #[test]
    fn reads_alternate_spellings() {
        assert_eq!(
            version_from_payload(&json!({"version": "1.2.3"})),
            Some("1.2.3")
        );
        assert_eq!(
            version_from_payload(&json!({"current_version": "1.2.3"})),
            Some("1.2.3")
        );
    }

    #[test]
    fn falls_back_to_lone_string_value() {
        assert_eq!(
            version_from_payload(&json!({"somethingElse": "9.9.9"})),
            Some("9.9.9")
        );
    }

    #[test]
    fn returns_none_when_ambiguous_or_missing() {
        assert_eq!(version_from_payload(&json!({})), None);
        assert_eq!(version_from_payload(&json!({"a": "1", "b": "2"})), None);
        assert_eq!(version_from_payload(&json!("2.13.0")), None);
    }
}
