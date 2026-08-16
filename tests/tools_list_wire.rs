//! `tools/list` over the real HTTP transport, in both protocol eras.
//!
//! The other suites validate tool *definitions* in-process, which cannot catch
//! a regression that only a client talking over the wire would see: a handshake
//! that yields no usable session, or an era whose request shape the server
//! mishandles. These tests perform the full client sequence and assert on the
//! response a client would actually parse.
//!
//! The two eras take genuinely different paths through rmcp:
//!
//! - **legacy** (<= 2025-11-25): `initialize` returns an `Mcp-Session-Id`, and
//!   every later request rides that session.
//! - **modern** (2026-07-28, SEP-2243/SEP-2575): sessionless. There is no
//!   session header; each request instead carries an `Mcp-Method` header and
//!   protocol metadata in `params._meta`, and results carry the `resultType` /
//!   `ttlMs` / `cacheScope` envelope.
//!
//! A server can be healthy on one era and broken on the other, so both are
//! covered. `scripts/inspector-verify.sh` runs the equivalent check against a
//! live binary using the official MCP Inspector client.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use pocket_id_mcp::client::PocketIdClient;
use pocket_id_mcp::config::{Config, HttpAuthMode};
use pocket_id_mcp::http::{HttpState, build_router};
use serde_json::json;
use tower::ServiceExt;

const LEGACY_VERSION: &str = "2025-11-25";
const MODERN_VERSION: &str = "2026-07-28";

fn make_router() -> axum::Router {
    let vars = HashMap::from([
        (
            "POCKET_ID_URL".to_string(),
            "https://id.example.com".to_string(),
        ),
        ("POCKET_ID_API_KEY".to_string(), "upstream-key".to_string()),
        ("POCKET_ID_MCP_TRANSPORT".to_string(), "http".to_string()),
        ("POCKET_ID_MCP_HTTP_AUTH".to_string(), "none".to_string()),
        (
            "POCKET_ID_MCP_ALLOW_DANGEROUS".to_string(),
            "true".to_string(),
        ),
    ]);
    let config = Arc::new(Config::from_vars(&vars).unwrap());
    let client = Arc::new(PocketIdClient::new("https://id.example.com", "k".into()));
    let state = match &config.http.as_ref().unwrap().auth {
        HttpAuthMode::None => HttpState::None,
        _ => unreachable!("configured as auth mode none"),
    };
    build_router(config, client, Arc::new(state))
}

fn post(headers: &[(&str, String)], body: serde_json::Value) -> Request<Body> {
    let mut req = Request::post("/mcp")
        .header(header::HOST, "localhost")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream");
    for (name, value) in headers {
        req = req.header(*name, value);
    }
    req.body(Body::from(body.to_string())).unwrap()
}

async fn body_text(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Responses may arrive as plain JSON or as an SSE frame; take the JSON-RPC
/// message either way.
fn json_rpc(text: &str) -> serde_json::Value {
    let payload = text
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .find(|line| line.contains("\"result\"") || line.contains("\"error\""))
        .unwrap_or_else(|| text.trim());
    serde_json::from_str(payload)
        .unwrap_or_else(|e| panic!("response was not JSON-RPC ({e}): {text}"))
}

fn initialize_body(version: &str) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": version,
            "capabilities": {},
            "clientInfo": {"name": "wire-test", "version": "0.0.0"}
        }
    })
}

/// Every tool a client is shown must be usable: named, and carrying an
/// object-rooted input schema. An empty list is treated as failure, since a
/// server that silently advertises nothing looks "healthy" to a smoke test.
fn assert_tools_usable(result: &serde_json::Value) {
    let tools = result["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("result has no tools array: {result}"));
    assert!(!tools.is_empty(), "tools/list returned an empty list");
    for tool in tools {
        let name = tool["name"].as_str().expect("tool has a name");
        assert!(!name.is_empty(), "tool with an empty name");
        assert_eq!(
            tool["inputSchema"]["type"].as_str(),
            Some("object"),
            "tool {name:?} has a non-object inputSchema root",
        );
    }
    assert!(
        tools
            .iter()
            .any(|t| t["name"].as_str() == Some("list_users")),
        "expected list_users in the advertised tool list",
    );
}

#[tokio::test]
async fn legacy_era_lists_tools_over_the_session() {
    let router = make_router();

    let resp = router
        .clone()
        .oneshot(post(&[], initialize_body(LEGACY_VERSION)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // The legacy handshake must hand back a session id; without one every
    // subsequent request is rejected as an unexpected message.
    let session = resp
        .headers()
        .get("mcp-session-id")
        .expect("legacy initialize must return an Mcp-Session-Id")
        .to_str()
        .unwrap()
        .to_string();

    let headers = vec![("mcp-session-id", session)];
    let resp = router
        .clone()
        .oneshot(post(
            &headers,
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    let resp = router
        .oneshot(post(
            &headers,
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "legacy tools/list must succeed"
    );
    let message = json_rpc(&body_text(resp).await);
    assert!(
        message.get("error").is_none(),
        "legacy tools/list returned an error: {message}"
    );
    assert_tools_usable(&message["result"]);
}

#[tokio::test]
async fn modern_era_lists_tools_without_a_session() {
    let router = make_router();

    // Modern is sessionless by design, so absence of the header is correct
    // here — the request shape below is what carries the context instead.
    let resp = router
        .clone()
        .oneshot(post(&[], initialize_body(MODERN_VERSION)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let negotiated = json_rpc(&body_text(resp).await);
    assert_eq!(
        negotiated["result"]["protocolVersion"].as_str(),
        Some(MODERN_VERSION),
        "server did not negotiate the modern revision",
    );

    let headers = vec![
        ("mcp-protocol-version", MODERN_VERSION.to_string()),
        ("mcp-method", "tools/list".to_string()),
    ];
    let resp = router
        .oneshot(post(
            &headers,
            json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/list",
                "params": {
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": MODERN_VERSION,
                        "io.modelcontextprotocol/clientCapabilities": {}
                    }
                }
            }),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "modern tools/list must succeed"
    );
    let message = json_rpc(&body_text(resp).await);
    assert!(
        message.get("error").is_none(),
        "modern tools/list returned an error: {message}"
    );
    let result = &message["result"];
    assert_tools_usable(result);
    // The modern revision requires the result envelope; a client that validates
    // strictly rejects the whole list when it is missing.
    assert_eq!(result["resultType"].as_str(), Some("complete"));
    assert!(result.get("ttlMs").is_some(), "modern result lacks ttlMs");
    assert!(
        result.get("cacheScope").is_some(),
        "modern result lacks cacheScope"
    );
}

#[tokio::test]
async fn modern_era_rejects_requests_missing_protocol_metadata() {
    // Negative control: the modern path's requirements are real, so a request
    // without them must fail. This keeps the test above honest — it proves the
    // metadata it sends is what makes it pass, not incidental leniency.
    let router = make_router();
    router
        .clone()
        .oneshot(post(&[], initialize_body(MODERN_VERSION)))
        .await
        .unwrap();

    let resp = router
        .oneshot(post(
            &[("mcp-protocol-version", MODERN_VERSION.to_string())],
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        ))
        .await
        .unwrap();
    let message = json_rpc(&body_text(resp).await);
    assert!(
        message.get("error").is_some(),
        "modern tools/list without protocol metadata should be rejected: {message}"
    );
}
