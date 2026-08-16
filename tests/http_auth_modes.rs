//! Integration tests for the non-OAuth HTTP auth modes: static shared bearer
//! token (`token`) and unauthenticated loopback (`none`). Covers admission,
//! challenge shape (no dead-end OAuth metadata pointers), absence of the
//! RFC 9728 metadata route, and retained DNS-rebinding protection.

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

const SECRET: &str = "local-shared-secret";

fn make_router(auth_mode: &str) -> axum::Router {
    let mut vars = HashMap::from([
        (
            "POCKET_ID_URL".to_string(),
            "https://id.example.com".to_string(),
        ),
        ("POCKET_ID_API_KEY".to_string(), "upstream-key".to_string()),
        ("POCKET_ID_MCP_TRANSPORT".to_string(), "http".to_string()),
        ("POCKET_ID_MCP_HTTP_AUTH".to_string(), auth_mode.to_string()),
    ]);
    if auth_mode == "token" {
        vars.insert("POCKET_ID_MCP_HTTP_TOKEN".to_string(), SECRET.to_string());
    }
    let config = Arc::new(Config::from_vars(&vars).unwrap());
    let client = Arc::new(PocketIdClient::new("https://id.example.com", "k".into()));
    let state = match &config.http.as_ref().unwrap().auth {
        HttpAuthMode::StaticToken { token } => HttpState::StaticToken {
            token: token.clone(),
        },
        HttpAuthMode::None => HttpState::None,
        HttpAuthMode::OAuth(_) => panic!("these tests cover non-OAuth modes"),
    };
    build_router(config, client, Arc::new(state))
}

fn initialize_request(bearer: Option<&str>, host: &str) -> Request<Body> {
    let mut req = Request::post("/mcp")
        .header(header::HOST, host)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream");
    if let Some(token) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    req.body(Body::from(
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test-client", "version": "0.0.0"}
            }
        })
        .to_string(),
    ))
    .unwrap()
}

async fn body_text(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn token_mode_admits_matching_token() {
    let router = make_router("token");
    let resp = router
        .oneshot(initialize_request(Some(SECRET), "localhost"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let text = body_text(resp).await;
    assert!(text.contains("pocket-id-mcp"), "got: {text}");
}

#[tokio::test]
async fn token_mode_rejects_wrong_and_missing_tokens() {
    for bearer in [Some("wrong-secret"), None] {
        let router = make_router("token");
        let resp = router
            .oneshot(initialize_request(bearer, "localhost"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "bearer {bearer:?}");
        let challenge = resp
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .expect("WWW-Authenticate header present")
            .to_str()
            .unwrap()
            .to_string();
        assert!(challenge.starts_with("Bearer "), "got: {challenge}");
        // No pointer to protected-resource metadata: there is no
        // authorization server to send a client to in this mode.
        assert!(!challenge.contains("resource_metadata"), "got: {challenge}");
    }
}

#[tokio::test]
async fn token_mode_serves_no_oauth_metadata() {
    let router = make_router("token");
    let resp = router
        .oneshot(
            Request::get("/.well-known/oauth-protected-resource")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn none_mode_processes_unauthenticated_requests() {
    let router = make_router("none");
    let resp = router
        .oneshot(initialize_request(None, "localhost"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let text = body_text(resp).await;
    assert!(text.contains("pocket-id-mcp"), "got: {text}");
}

#[tokio::test]
async fn none_mode_serves_no_oauth_metadata() {
    let router = make_router("none");
    let resp = router
        .oneshot(
            Request::get("/.well-known/oauth-protected-resource")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn none_mode_keeps_dns_rebinding_protection() {
    let router = make_router("none");
    let resp = router
        .oneshot(initialize_request(None, "evil.example.com"))
        .await
        .unwrap();
    assert!(
        resp.status().is_client_error(),
        "spoofed Host must be rejected, got {}",
        resp.status()
    );
}
