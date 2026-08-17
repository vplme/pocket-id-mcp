//! Logging tests: what is recorded for HTTP requests and tool calls, and —
//! more importantly — what must never appear. The prohibitions here are the
//! only thing making "log every call, reads included" safe, since read-tier
//! tools return credential material.

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use pocket_id_mcp::client::PocketIdClient;
use pocket_id_mcp::config::{Config, HttpAuthMode};
use pocket_id_mcp::http::{HttpState, build_router};
use serde_json::json;
use tower::ServiceExt;
use tracing_subscriber::layer::SubscriberExt;

const SECRET: &str = "local-shared-secret";

/// Log sink that accumulates everything written, for assertions.
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

/// Run `body` with logs captured under the crate's default filter, then return
/// everything logged. Uses a scoped (not global) subscriber so tests do not
/// contend for the process-wide default.
async fn capture_logs<F, Fut>(body: F) -> String
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let logs = CapturedLogs::default();
    // The production default filter, so these tests fail if a record is only
    // visible once an operator sets RUST_LOG.
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("pocket_id_mcp=info"))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(logs.clone())
                .with_ansi(false),
        );
    let guard = tracing::subscriber::set_default(subscriber);
    body().await;
    drop(guard);
    logs.text()
}

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

fn mcp_request(bearer: Option<&str>, payload: serde_json::Value) -> Request<Body> {
    let mut req = Request::post("/mcp")
        .header(header::HOST, "localhost")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream");
    if let Some(token) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    req.body(Body::from(payload.to_string())).unwrap()
}

fn initialize_payload() -> serde_json::Value {
    json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "0.0.0"}
        }
    })
}

/// Drive a full session: initialize, then a `tools/call`, reusing the session
/// id the server hands back. Returns the tool call's response body.
async fn call_tool_over_http(
    router: axum::Router,
    bearer: Option<&str>,
    tool: &str,
    arguments: serde_json::Value,
) -> String {
    let init = router
        .clone()
        .oneshot(mcp_request(bearer, initialize_payload()))
        .await
        .unwrap();
    assert_eq!(init.status(), StatusCode::OK);
    let session = init
        .headers()
        .get("mcp-session-id")
        .map(|v| v.to_str().unwrap().to_string());

    let mut req = Request::post("/mcp")
        .header(header::HOST, "localhost")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream");
    if let Some(token) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(id) = &session {
        req = req.header("mcp-session-id", id.as_str());
    }
    let payload = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": tool, "arguments": arguments }
    });
    let resp = router
        .oneshot(req.body(Body::from(payload.to_string())).unwrap())
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn access_record_emitted_for_admitted_request() {
    let logs = capture_logs(|| async {
        let resp = make_router("token")
            .oneshot(mcp_request(Some(SECRET), initialize_payload()))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    })
    .await;

    assert!(logs.contains("http_request"), "no access record: {logs}");
    assert!(logs.contains("method=POST"), "no method: {logs}");
    assert!(logs.contains("path=/mcp"), "no path: {logs}");
    assert!(logs.contains("200"), "no status: {logs}");
}

#[tokio::test]
async fn rejected_request_is_logged_and_reaches_no_tool() {
    let logs = capture_logs(|| async {
        let resp = make_router("token")
            .oneshot(mcp_request(Some("wrong-secret"), initialize_payload()))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    })
    .await;

    // The probe is visible...
    assert!(logs.contains("http_request"), "no access record: {logs}");
    assert!(logs.contains("401"), "401 not recorded: {logs}");
    // ...and nothing dispatched.
    assert!(!logs.contains("tool call"), "tool ran anyway: {logs}");
}

#[tokio::test]
async fn admitted_caller_is_attributed_without_leaking_the_secret() {
    let logs = capture_logs(|| async {
        let resp = make_router("token")
            .oneshot(mcp_request(Some(SECRET), initialize_payload()))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    })
    .await;

    assert!(logs.contains("static-token"), "no actor: {logs}");
    assert!(
        !logs.contains(SECRET),
        "the shared secret reached the log: {logs}"
    );
}

#[tokio::test]
async fn unauthenticated_mode_records_no_actor() {
    let logs = capture_logs(|| async {
        let resp = make_router("none")
            .oneshot(mcp_request(None, initialize_payload()))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    })
    .await;

    assert!(logs.contains("http_request"), "no access record: {logs}");
    assert!(!logs.contains("actor="), "actor invented: {logs}");
}

#[tokio::test]
async fn tool_call_records_name_tier_and_allowlisted_params() {
    let logs = capture_logs(|| async {
        // A dangerous-tier tool is unregistered by default, so use a read tool
        // whose identifying parameter is allowlisted. The upstream call fails
        // (no server), which is fine: the record is emitted either way.
        call_tool_over_http(
            make_router("none"),
            None,
            "get_user",
            json!({ "user_id": "user-abc-123" }),
        )
        .await;
    })
    .await;

    assert!(logs.contains("tool call"), "no tool record: {logs}");
    assert!(logs.contains("tool=get_user"), "no tool name: {logs}");
    assert!(logs.contains(r#"tier="read""#), "no tier: {logs}");
    // The parameter is its own namespaced field, not text inside another one.
    assert!(
        logs.contains(r#"params.user_id="user-abc-123""#),
        "allowlisted param missing or not namespaced: {logs}"
    );
}

#[tokio::test]
async fn tool_call_without_allowlisted_params_emits_no_param_fields() {
    let logs = capture_logs(|| async {
        // `list_users` takes only paging and search params, none allowlisted.
        call_tool_over_http(make_router("none"), None, "list_users", json!({})).await;
    })
    .await;

    assert!(logs.contains("tool call"), "no tool record: {logs}");
    // Unset fields must not render at all — no empty `params` placeholder.
    assert!(
        !logs.contains("params."),
        "unset parameter fields rendered: {logs}"
    );
}

#[tokio::test]
async fn tool_call_record_omits_secret_bearing_arguments() {
    const TOKEN: &str = "super-secret-bearer-value";
    let logs = capture_logs(|| async {
        call_tool_over_http(
            make_router("none"),
            None,
            "introspect_token",
            json!({ "token": TOKEN }),
        )
        .await;
    })
    .await;

    assert!(logs.contains("tool call"), "no tool record: {logs}");
    assert!(
        logs.contains("tool=introspect_token"),
        "no tool name: {logs}"
    );
    assert!(
        !logs.contains(TOKEN),
        "a bearer token reached the log: {logs}"
    );
}

#[tokio::test]
async fn upstream_response_content_never_reaches_the_log() {
    // A real Pocket ID instance would answer this read with LDAP and SMTP
    // credentials. Point the client at a wiremock server returning exactly
    // that shape and assert none of it is logged.
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/api/application-configuration/all",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!([
            { "key": "ldapBindPassword", "value": "ldap-secret-value" },
            { "key": "smtpPassword", "value": "smtp-secret-value" },
        ])))
        .mount(&mock)
        .await;

    let vars = HashMap::from([
        ("POCKET_ID_URL".to_string(), mock.uri()),
        ("POCKET_ID_API_KEY".to_string(), "upstream-key".to_string()),
        ("POCKET_ID_MCP_TRANSPORT".to_string(), "http".to_string()),
        ("POCKET_ID_MCP_HTTP_AUTH".to_string(), "none".to_string()),
    ]);
    let config = Arc::new(Config::from_vars(&vars).unwrap());
    let client = Arc::new(PocketIdClient::new(
        &config.pocket_id_url,
        config.api_key.clone(),
    ));
    let router = build_router(config, client, Arc::new(HttpState::None));

    let logs = capture_logs(|| async {
        let body =
            call_tool_over_http(router, None, "get_all_application_configuration", json!({})).await;
        // The client really did receive the secrets — so the log's silence
        // below is meaningful, not an artifact of a failed call.
        assert!(body.contains("ldap-secret-value"), "got: {body}");
    })
    .await;

    assert!(logs.contains("tool call"), "no tool record: {logs}");
    assert!(
        !logs.contains("ldap-secret-value"),
        "response content leaked into the log: {logs}"
    );
    assert!(
        !logs.contains("smtp-secret-value"),
        "response content leaked into the log: {logs}"
    );
}

#[tokio::test]
async fn unknown_tool_is_logged_and_reported_to_the_client() {
    let logs = capture_logs(|| async {
        let body = call_tool_over_http(make_router("none"), None, "no_such_tool", json!({})).await;
        // Dispatch behavior is unchanged by the hand-written call_tool: the
        // client still gets an error for an unregistered name.
        assert!(body.contains("error"), "got: {body}");
    })
    .await;

    assert!(logs.contains("tool call"), "no tool record: {logs}");
    assert!(logs.contains("tool=no_such_tool"), "no tool name: {logs}");
    // Absent from the catalog, so no tier can be resolved.
    assert!(
        logs.contains(r#"tier="unknown""#),
        "unexpected tier: {logs}"
    );
}

#[tokio::test]
async fn json_format_emits_parseable_records() {
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("pocket_id_mcp=info"))
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(logs.clone())
                .with_ansi(false),
        );
    let guard = tracing::subscriber::set_default(subscriber);
    call_tool_over_http(
        make_router("none"),
        None,
        "get_user",
        json!({ "user_id": "user-abc-123" }),
    )
    .await;
    drop(guard);

    let text = logs.text();
    let tool_record = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|v| {
            v.get("fields")
                .and_then(|f| f.get("message"))
                .and_then(|m| m.as_str())
                == Some("tool call")
        })
        .unwrap_or_else(|| panic!("no JSON tool record in: {text}"));

    // Structured fields must survive as queryable JSON keys, not be baked
    // into the message string.
    let fields = tool_record.get("fields").unwrap();
    assert_eq!(fields.get("tool").unwrap(), "get_user");
    assert_eq!(fields.get("tier").unwrap(), "read");
    assert_eq!(fields.get("outcome").unwrap(), "error");
    assert!(fields.get("duration_ms").is_some(), "no duration");

    // Parameters ride on the enclosing span, each as its own namespaced key —
    // queryable directly rather than parsed out of an encoded string.
    let spans = tool_record
        .get("spans")
        .and_then(|s| s.as_array())
        .unwrap_or_else(|| panic!("no spans in: {tool_record}"));
    let params_span = spans
        .iter()
        .find(|s| s.get("params.user_id").is_some())
        .unwrap_or_else(|| panic!("no param span in: {tool_record}"));
    assert_eq!(params_span.get("params.user_id").unwrap(), "user-abc-123");
    // Declared but unrecorded slots must not appear.
    assert!(
        params_span.get("params.group_id").is_none(),
        "unset parameter rendered: {params_span}"
    );
}
