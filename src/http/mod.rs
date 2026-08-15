//! Streamable HTTP transport secured as an OAuth 2.1 protected resource
//! (RFC 9728 metadata, bearer validation, group admission).

pub mod auth;

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};

use crate::client::PocketIdClient;
use crate::config::Config;
use crate::server::PocketIdServer;
use auth::{AuthError, Authenticator};

pub struct HttpState {
    pub authenticator: Authenticator,
    /// Absolute URL of the protected resource metadata document.
    pub metadata_url: String,
    pub resource: String,
    pub issuer: String,
}

/// RFC 9728: insert the well-known segment between origin and path.
pub fn metadata_url_for(public_url: &str) -> String {
    match url::Url::parse(public_url) {
        Ok(u) => {
            let origin = format!(
                "{}://{}{}",
                u.scheme(),
                u.host_str().unwrap_or_default(),
                u.port().map(|p| format!(":{p}")).unwrap_or_default()
            );
            let path = u.path().trim_end_matches('/');
            format!("{origin}/.well-known/oauth-protected-resource{path}")
        }
        Err(_) => format!("{public_url}/.well-known/oauth-protected-resource"),
    }
}

async fn protected_resource_metadata(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "resource": state.resource,
        "authorization_servers": [state.issuer],
        "bearer_methods_supported": ["header"],
        "resource_name": "pocket-id-mcp",
    }))
}

fn unauthorized(state: &HttpState, reason: &str) -> Response {
    let challenge = format!(
        "Bearer resource_metadata=\"{}\", error=\"invalid_token\", error_description=\"{}\"",
        state.metadata_url,
        reason.replace('"', "'")
    );
    let mut resp = (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "invalid_token", "error_description": reason })),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&challenge) {
        resp.headers_mut().insert(header::WWW_AUTHENTICATE, value);
    }
    resp
}

async fn auth_middleware(
    State(state): State<Arc<HttpState>>,
    request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|t| !t.is_empty());

    let Some(token) = token else {
        return unauthorized(&state, "missing bearer token");
    };

    match state.authenticator.validate(token).await {
        Ok(_claims) => next.run(request).await,
        Err(AuthError::Unauthorized(reason)) => unauthorized(&state, &reason),
        Err(AuthError::Forbidden(reason)) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "insufficient_access",
                "error_description": reason,
            })),
        )
            .into_response(),
        Err(AuthError::Internal(reason)) => {
            tracing::error!(%reason, "token validation infrastructure failure");
            unauthorized(&state, "token validation unavailable")
        }
    }
}

/// Hosts accepted by the MCP endpoint (DNS-rebinding protection): loopback
/// plus the public URL's host and the bind address.
fn allowed_hosts(config: &Config) -> Vec<String> {
    let mut hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    if let Some(http) = &config.http {
        hosts.push(http.bind.clone());
        if let Some(host) = http.bind.rsplit_once(':').map(|(h, _)| h.to_string()) {
            hosts.push(host);
        }
        if let Ok(u) = url::Url::parse(&http.public_url) {
            if let Some(host) = u.host_str() {
                hosts.push(host.to_string());
                if let Some(port) = u.port() {
                    hosts.push(format!("{host}:{port}"));
                }
            }
        }
    }
    hosts.sort_unstable();
    hosts.dedup();
    hosts
}

/// Build the complete HTTP router (exposed separately for integration tests).
pub fn build_router(
    config: Arc<Config>,
    client: Arc<PocketIdClient>,
    state: Arc<HttpState>,
) -> Router {
    let hosts = allowed_hosts(&config);
    let mcp_service = StreamableHttpService::new(
        move || Ok(PocketIdServer::new(config.clone(), client.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(hosts)
            // Plain JSON responses on POST (allowed by the MCP spec) instead of
            // SSE streams: simpler for clients and proxies alike.
            .with_json_response(true),
    );

    let protected = Router::new().nest_service("/mcp", mcp_service).layer(
        axum::middleware::from_fn_with_state(state.clone(), auth_middleware),
    );

    Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/{*path}",
            get(protected_resource_metadata),
        )
        .merge(protected)
        .with_state(state)
}

/// Construct state, run startup validation, and serve until ctrl-c.
pub async fn serve(config: Arc<Config>, client: Arc<PocketIdClient>) -> anyhow::Result<()> {
    let http_config = config
        .http
        .clone()
        .ok_or_else(|| anyhow::anyhow!("HTTP transport selected without HTTP configuration"))?;

    let authenticator = Authenticator::new(
        http_config.clone(),
        config.pocket_id_url.clone(),
        client.clone(),
    );

    let discovery = authenticator.init().await.map_err(|e| {
        anyhow::anyhow!(
            "OAuth issuer validation failed for {}: {e}",
            http_config.oauth_issuer
        )
    })?;
    tracing::info!(
        issuer = %discovery.issuer,
        jwks = %discovery.jwks_uri,
        resource = %http_config.public_url,
        "OAuth resource server initialized"
    );

    let state = Arc::new(HttpState {
        metadata_url: metadata_url_for(&http_config.public_url),
        resource: http_config.public_url.clone(),
        issuer: http_config.oauth_issuer.clone(),
        authenticator,
    });

    let router = build_router(config.clone(), client, state);
    let listener = tokio::net::TcpListener::bind(&http_config.bind).await?;
    tracing::info!(bind = %http_config.bind, "serving MCP over streamable HTTP at /mcp");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
