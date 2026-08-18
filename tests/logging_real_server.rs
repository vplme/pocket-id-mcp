//! Logging over a real socket, with a real MCP session.
//!
//! `tests/logging.rs` drives the router with `oneshot`, which runs everything
//! inline on the calling task. That is not how the server actually serves:
//! rmcp hands each handler to a detached task, and in session mode to a worker
//! created by an earlier request. Anything that depends on task-local state —
//! `tracing`'s current span, above all — can behave differently there.
//!
//! This file exists to catch that class of difference, so it binds a port and
//! speaks HTTP. It is a separate test binary because it installs a *global*
//! subscriber: the handler runs on a task this test does not own, so a scoped
//! subscriber would not reach it, and a global one would otherwise leak into
//! every other test sharing the binary.

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};

use pocket_id_mcp::client::PocketIdClient;
use pocket_id_mcp::config::Config;
use pocket_id_mcp::http::{HttpState, build_router};
use serde_json::json;
use tracing_subscriber::layer::SubscriberExt;

#[derive(Clone, Default)]
struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

impl CapturedLogs {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

impl io::Write for CapturedLogs {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLogs {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Serve on an ephemeral port, run one `tools/call`, return everything logged.
async fn logs_from_real_tool_call(tool: &str, arguments: serde_json::Value) -> String {
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::registry()
        // The production default filter: a record only visible under RUST_LOG
        // would not be much of an audit trail.
        .with(tracing_subscriber::EnvFilter::new("pocket_id_mcp=info"))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(logs.clone())
                .with_ansi(false),
        );
    let _ = tracing::subscriber::set_global_default(subscriber);

    let vars = HashMap::from([
        (
            "POCKET_ID_URL".to_string(),
            "https://id.example.com".to_string(),
        ),
        ("POCKET_ID_API_KEY".to_string(), "upstream-key".to_string()),
        ("POCKET_ID_MCP_TRANSPORT".to_string(), "http".to_string()),
        ("POCKET_ID_MCP_HTTP_AUTH".to_string(), "none".to_string()),
    ]);
    let config = Arc::new(Config::from_vars(&vars).unwrap());
    let client = Arc::new(PocketIdClient::new("https://id.example.com", "k".into()));
    let router = build_router(config, client, Arc::new(HttpState::None));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    let http = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let init = http
        .post(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "test-client", "version": "0.0.0"}
                }
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();
    let session = init
        .headers()
        .get("mcp-session-id")
        .map(|v| v.to_str().unwrap().to_string());

    let mut call = http
        .post(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    if let Some(id) = &session {
        call = call.header("mcp-session-id", id.as_str());
    }
    let _ = call
        .body(
            json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": tool, "arguments": arguments }
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    server.abort();

    logs.text()
}

#[tokio::test]
async fn params_survive_the_real_serving_path() {
    let logs = logs_from_real_tool_call(
        "list_user_groups_of_user",
        json!({ "user_id": "user-abc-123" }),
    )
    .await;

    assert!(logs.contains("tool call"), "no tool record: {logs}");
    assert!(
        logs.contains("tool=list_user_groups_of_user"),
        "no tool name: {logs}"
    );
    // The parameter must reach the record through the real dispatch path, not
    // only through `oneshot`'s inline one.
    assert!(
        logs.contains(r#"params.user_id="user-abc-123""#),
        "parameter lost on the real serving path: {logs}"
    );
    // And the access record for the same request is present alongside it.
    assert!(logs.contains("http_request"), "no access record: {logs}");
}
