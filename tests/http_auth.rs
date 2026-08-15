//! HTTP-mode integration tests: RFC 9728 metadata, 401 challenges, audience
//! rejection, group admission, introspection fallback, and a happy-path MCP
//! tool call that proves the client bearer token is never forwarded upstream.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use pocket_id_mcp::client::PocketIdClient;
use pocket_id_mcp::config::Config;
use pocket_id_mcp::http::auth::{AuthError, Authenticator};
use pocket_id_mcp::http::{HttpState, build_router, metadata_url_for};
use serde_json::json;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Match, Mock, MockServer, ResponseTemplate};

const KID: &str = "test-key";
const RESOURCE: &str = "https://mcp.example.com/mcp";

/// Throwaway RSA signing key for the mock issuer, generated once per test run
/// so no key material lives in the repository.
struct TestKey {
    /// PKCS#8 PEM of the private key, for minting JWTs.
    pem: String,
    /// base64url modulus for the mock JWKS (exponent is the standard AQAB).
    n_b64: String,
}

fn test_key() -> &'static TestKey {
    static KEY: OnceLock<TestKey> = OnceLock::new();
    KEY.get_or_init(|| {
        use base64::Engine;
        use rsa::pkcs8::EncodePrivateKey;
        use rsa::traits::PublicKeyParts;
        let key =
            rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).expect("RSA key generation");
        TestKey {
            pem: key
                .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
                .expect("PEM export")
                .to_string(),
            n_b64: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.n().to_bytes_be()),
        }
    })
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Mount OIDC discovery + JWKS on a mock authorization server.
async fn mount_issuer(server: &MockServer) {
    let issuer = server.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "issuer": issuer,
            "jwks_uri": format!("{issuer}/jwks.json"),
            "authorization_endpoint": format!("{issuer}/authorize"),
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "keys": [{
                "kty": "RSA",
                "kid": KID,
                "use": "sig",
                "alg": "RS256",
                "n": test_key().n_b64,
                "e": "AQAB",
            }]
        })))
        .mount(server)
        .await;
}

fn mint_token(issuer: &str, aud: &str, groups: Option<Vec<&str>>, exp_offset: i64) -> String {
    let mut claims = json!({
        "iss": issuer,
        "aud": aud,
        "sub": "user-1",
        "exp": (now() as i64 + exp_offset),
        "iat": now(),
    });
    if let Some(groups) = groups {
        claims["groups"] = json!(groups);
    }
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(KID.to_string());
    encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(test_key().pem.as_bytes()).unwrap(),
    )
    .unwrap()
}

fn make_config(pocket_id_url: &str, issuer: &str, allowed_groups: Option<&str>) -> Arc<Config> {
    let mut vars = HashMap::from([
        ("POCKET_ID_URL".to_string(), pocket_id_url.to_string()),
        ("POCKET_ID_API_KEY".to_string(), "upstream-key".to_string()),
        ("POCKET_ID_MCP_TRANSPORT".to_string(), "http".to_string()),
        ("POCKET_ID_MCP_PUBLIC_URL".to_string(), RESOURCE.to_string()),
        ("POCKET_ID_MCP_OAUTH_ISSUER".to_string(), issuer.to_string()),
    ]);
    if let Some(groups) = allowed_groups {
        vars.insert(
            "POCKET_ID_MCP_ALLOWED_GROUPS".to_string(),
            groups.to_string(),
        );
    }
    Arc::new(Config::from_vars(&vars).unwrap())
}

fn make_state(config: &Arc<Config>, client: &Arc<PocketIdClient>) -> Arc<HttpState> {
    let http_config = config.http.clone().unwrap();
    Arc::new(HttpState {
        metadata_url: metadata_url_for(&http_config.public_url),
        resource: http_config.public_url.clone(),
        issuer: http_config.oauth_issuer.clone(),
        authenticator: Authenticator::new(
            http_config,
            config.pocket_id_url.clone(),
            client.clone(),
        ),
    })
}

fn make_router(config: Arc<Config>, client: Arc<PocketIdClient>) -> (axum::Router, Arc<HttpState>) {
    let state = make_state(&config, &client);
    (build_router(config, client, state.clone()), state)
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(json!(null))
}

#[tokio::test]
async fn unauthenticated_request_gets_challenge_with_metadata() {
    let issuer = MockServer::start().await;
    mount_issuer(&issuer).await;
    let config = make_config("https://id.example.com", &issuer.uri(), None);
    let client = Arc::new(PocketIdClient::new("https://id.example.com", "k".into()));
    let (router, _state) = make_router(config, client);

    let resp = router
        .oneshot(
            Request::post("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let challenge = resp
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .expect("WWW-Authenticate header present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(challenge.starts_with("Bearer "), "got: {challenge}");
    assert!(
        challenge.contains(
            "resource_metadata=\"https://mcp.example.com/.well-known/oauth-protected-resource/mcp\""
        ),
        "got: {challenge}"
    );
}

#[tokio::test]
async fn protected_resource_metadata_served() {
    let issuer = MockServer::start().await;
    mount_issuer(&issuer).await;
    let config = make_config("https://id.example.com", &issuer.uri(), None);
    let client = Arc::new(PocketIdClient::new("https://id.example.com", "k".into()));
    let (router, _state) = make_router(config, client);

    for uri in [
        "/.well-known/oauth-protected-resource",
        "/.well-known/oauth-protected-resource/mcp",
    ] {
        let resp = router
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "at {uri}");
        let body = body_json(resp).await;
        assert_eq!(body["resource"], RESOURCE, "at {uri}");
        assert_eq!(body["authorization_servers"][0], issuer.uri(), "at {uri}");
    }
}

#[tokio::test]
async fn wrong_audience_rejected() {
    let issuer = MockServer::start().await;
    mount_issuer(&issuer).await;
    let config = make_config("https://id.example.com", &issuer.uri(), None);
    let client = Arc::new(PocketIdClient::new("https://id.example.com", "k".into()));
    let (router, _state) = make_router(config, client);

    let token = mint_token(
        &issuer.uri(),
        "https://some-other-api.example.com",
        None,
        3600,
    );
    let resp = router
        .oneshot(
            Request::post("/mcp")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(resp).await;
    assert!(
        body["error_description"]
            .as_str()
            .unwrap_or_default()
            .contains("audience"),
        "got: {body}"
    );
}

#[tokio::test]
async fn expired_token_rejected() {
    let issuer = MockServer::start().await;
    mount_issuer(&issuer).await;
    let config = make_config("https://id.example.com", &issuer.uri(), None);
    let client = Arc::new(PocketIdClient::new("https://id.example.com", "k".into()));
    let (router, _state) = make_router(config, client);

    let token = mint_token(&issuer.uri(), RESOURCE, None, -3600);
    let resp = router
        .oneshot(
            Request::post("/mcp")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn group_admission_enforced() {
    let issuer = MockServer::start().await;
    mount_issuer(&issuer).await;
    let config = make_config("https://id.example.com", &issuer.uri(), Some("admins"));
    let client = Arc::new(PocketIdClient::new("https://id.example.com", "k".into()));
    let (router, state) = make_router(config, client);

    // Token without the required group → 403.
    let outsider = mint_token(&issuer.uri(), RESOURCE, Some(vec!["users"]), 3600);
    let resp = router
        .clone()
        .oneshot(
            Request::post("/mcp")
                .header(header::AUTHORIZATION, format!("Bearer {outsider}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Token with the group passes validation.
    let admin = mint_token(&issuer.uri(), RESOURCE, Some(vec!["admins", "users"]), 3600);
    let claims = state.authenticator.validate(&admin).await.unwrap();
    assert_eq!(claims["sub"], "user-1");
}

#[tokio::test]
async fn opaque_token_falls_back_to_pocket_id_introspection() {
    // The mock server plays both the Pocket ID upstream and the issuer.
    let pocket = MockServer::start().await;
    mount_issuer(&pocket).await;
    Mock::given(method("POST"))
        .and(path("/api/oidc/introspect"))
        .and(wiremock::matchers::header("X-API-KEY", "upstream-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "active": true,
            "sub": "user-2",
            "aud": RESOURCE,
            "groups": ["admins"],
        })))
        .expect(1)
        .mount(&pocket)
        .await;

    let config = make_config(&pocket.uri(), &pocket.uri(), Some("admins"));
    let client = Arc::new(PocketIdClient::new(&pocket.uri(), "upstream-key".into()));
    let state = make_state(&config, &client);

    let claims = state
        .authenticator
        .validate("opaque-token-value")
        .await
        .unwrap();
    assert_eq!(claims["sub"], "user-2");
}

#[tokio::test]
async fn opaque_token_rejected_for_external_issuer() {
    let issuer = MockServer::start().await;
    mount_issuer(&issuer).await;
    // Issuer differs from the Pocket ID URL → no introspection fallback.
    let config = make_config("https://id.example.com", &issuer.uri(), None);
    let client = Arc::new(PocketIdClient::new("https://id.example.com", "k".into()));
    let state = make_state(&config, &client);

    let err = state
        .authenticator
        .validate("opaque-token")
        .await
        .unwrap_err();
    assert!(matches!(err, AuthError::Unauthorized(_)));
}

/// Matcher asserting the upstream request does NOT carry an Authorization
/// header (no token passthrough), on top of carrying the API key.
struct NoAuthorizationHeader;

impl Match for NoAuthorizationHeader {
    fn matches(&self, request: &wiremock::Request) -> bool {
        !request.headers.contains_key("authorization")
    }
}

#[tokio::test]
async fn happy_path_tool_call_without_token_passthrough() {
    let issuer = MockServer::start().await;
    mount_issuer(&issuer).await;

    // Upstream Pocket ID mock: must see the API key, must never see the bearer.
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .and(wiremock::matchers::header("X-API-KEY", "upstream-key"))
        .and(NoAuthorizationHeader)
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&upstream)
        .await;

    let config = make_config(&upstream.uri(), &issuer.uri(), None);
    let client = Arc::new(PocketIdClient::new(&upstream.uri(), "upstream-key".into()));
    let (router, _state) = make_router(config, client);
    let token = mint_token(&issuer.uri(), RESOURCE, None, 3600);

    let rpc = |body: serde_json::Value, session: Option<&str>| {
        let mut req = Request::post("/mcp")
            .header(header::HOST, "mcp.example.com")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream");
        if let Some(s) = session {
            req = req.header("Mcp-Session-Id", s);
        }
        req.body(Body::from(body.to_string())).unwrap()
    };

    // initialize → session id
    let resp = router
        .clone()
        .oneshot(rpc(
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "test-client", "version": "0.0.0"}
                }
            }),
            None,
        ))
        .await
        .unwrap();
    let status = resp.status();
    if status != StatusCode::OK {
        let hdrs = format!("{:?}", resp.headers());
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        panic!(
            "initialize failed: {status} {hdrs} {}",
            String::from_utf8_lossy(&body)
        );
    }
    let session = resp
        .headers()
        .get("mcp-session-id")
        .expect("session id issued")
        .to_str()
        .unwrap()
        .to_string();

    // initialized notification
    let resp = router
        .clone()
        .oneshot(rpc(
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            Some(&session),
        ))
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "initialized notification failed"
    );

    // tools/call health_check → drives an upstream request
    let resp = router
        .clone()
        .oneshot(rpc(
            json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {"name": "health_check", "arguments": {}}
            }),
            Some(&session),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "tools/call failed");
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("ok"),
        "tool result missing upstream body: {text}"
    );
    // upstream .expect(1) verifies the call happened; NoAuthorizationHeader
    // verifies the bearer token was not forwarded.
}
