//! Streamable HTTP transport with selectable authentication: OAuth 2.1
//! protected resource (RFC 9728 metadata, bearer validation, group admission),
//! static shared bearer token, or unauthenticated (loopback-guarded).

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
use subtle::ConstantTimeEq;

use crate::client::PocketIdClient;
use crate::config::{Config, HttpAuthMode};
use crate::server::PocketIdServer;
use auth::{AuthError, Authenticator};

pub struct OAuthState {
    pub authenticator: Authenticator,
    /// Absolute URL of the protected resource metadata document.
    pub metadata_url: String,
    pub resource: String,
    pub issuer: String,
}

/// Per-mode router state; OAuth-only machinery exists only in the OAuth arm.
pub enum HttpState {
    OAuth(Box<OAuthState>),
    StaticToken { token: String },
    None,
}

impl HttpState {
    /// OAuth-mode state, when that is the active mode.
    pub fn oauth(&self) -> Option<&OAuthState> {
        match self {
            HttpState::OAuth(o) => Some(o.as_ref()),
            _ => None,
        }
    }
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

async fn protected_resource_metadata(State(state): State<Arc<HttpState>>) -> Response {
    let Some(oauth) = state.oauth() else {
        // Route is only mounted in OAuth mode.
        return StatusCode::NOT_FOUND.into_response();
    };
    Json(serde_json::json!({
        "resource": oauth.resource,
        "authorization_servers": [oauth.issuer],
        "bearer_methods_supported": ["header"],
        "resource_name": "pocket-id-mcp",
    }))
    .into_response()
}

fn bearer_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

fn unauthorized(challenge: &str, reason: &str) -> Response {
    let mut resp = (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "invalid_token", "error_description": reason })),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(challenge) {
        resp.headers_mut().insert(header::WWW_AUTHENTICATE, value);
    }
    resp
}

fn oauth_unauthorized(oauth: &OAuthState, reason: &str) -> Response {
    let challenge = format!(
        "Bearer resource_metadata=\"{}\", error=\"invalid_token\", error_description=\"{}\"",
        oauth.metadata_url,
        reason.replace('"', "'")
    );
    unauthorized(&challenge, reason)
}

async fn oauth_middleware(
    State(state): State<Arc<HttpState>>,
    request: Request,
    next: Next,
) -> Response {
    let Some(oauth) = state.oauth() else {
        // Only wired in OAuth mode; never reached otherwise.
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let Some(token) = bearer_token(&request) else {
        return oauth_unauthorized(oauth, "missing bearer token");
    };

    match oauth.authenticator.validate(token).await {
        Ok(claims) => {
            // The subject is the only claim logged: it identifies the caller
            // without carrying the token or any other claim content.
            record_actor(
                claims
                    .get("sub")
                    .and_then(|s| s.as_str())
                    .unwrap_or("(no subject)"),
            );
            next.run(request).await
        }
        Err(AuthError::Unauthorized(reason)) => oauth_unauthorized(oauth, &reason),
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
            oauth_unauthorized(oauth, "token validation unavailable")
        }
    }
}

/// Static shared-secret admission: no issuer, no metadata, no OAuth challenge
/// (a `resource_metadata` pointer would send clients on a dead-end OAuth dance).
async fn static_token_middleware(
    State(state): State<Arc<HttpState>>,
    request: Request,
    next: Next,
) -> Response {
    let HttpState::StaticToken { token: expected } = state.as_ref() else {
        // Only wired in static-token mode; never reached otherwise.
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let presented = bearer_token(&request).unwrap_or_default();
    if presented.as_bytes().ct_eq(expected.as_bytes()).into() {
        // Every caller shares one secret here, so there is no identity to
        // report — a fixed label is the honest answer, and the secret itself
        // is never logged.
        record_actor("static-token");
        next.run(request).await
    } else {
        let reason = if presented.is_empty() {
            "missing bearer token"
        } else {
            "invalid bearer token"
        };
        unauthorized(
            &format!("Bearer error=\"invalid_token\", error_description=\"{reason}\""),
            reason,
        )
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
/// Host-header validation stays active in every mode — in `none` mode it is
/// the only remaining request-level defense against DNS rebinding.
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
    let mcp = Router::new().nest_service("/mcp", mcp_service);

    let routed = match state.as_ref() {
        HttpState::OAuth(_) => Router::new()
            .route(
                "/.well-known/oauth-protected-resource",
                get(protected_resource_metadata),
            )
            .route(
                "/.well-known/oauth-protected-resource/{*path}",
                get(protected_resource_metadata),
            )
            .with_state(state.clone())
            .merge(mcp.layer(axum::middleware::from_fn_with_state(
                state,
                oauth_middleware,
            ))),
        // route_layer, not layer: unknown paths must fall through to a plain
        // 404 rather than an auth challenge for a route that doesn't exist.
        HttpState::StaticToken { .. } => mcp.route_layer(axum::middleware::from_fn_with_state(
            state,
            static_token_middleware,
        )),
        HttpState::None => mcp,
    };

    // Outermost, so requests rejected by auth are logged too: repeated 401s
    // and 403s are the visible signature of someone probing a server that
    // holds an admin API key.
    routed.layer(access_log_layer())
}

/// Field name under which each middleware records the admitted caller.
const ACTOR_FIELD: &str = "actor";

/// Record the caller on the current request span.
///
/// Called only after admission, so an unauthenticated request is never
/// attributed. Never receives a token or the shared secret itself — OAuth
/// passes the subject claim, static-token mode a fixed label.
fn record_actor(actor: &str) {
    tracing::Span::current().record(ACTOR_FIELD, actor);
}

/// Per-request span and completion record for every HTTP request.
///
/// The span exists before authentication runs, so `actor` is declared empty
/// and filled in by whichever middleware admits the request.
///
/// Both the span and the completion event are emitted under this crate's
/// target. `tower_http`'s built-in `DefaultOnResponse` would emit under
/// `tower_http`'s own module path and at DEBUG, so the default
/// `pocket_id_mcp=info` filter would drop every access record — an access log
/// invisible without `RUST_LOG` is not an access log. The response fields are
/// therefore emitted by hand rather than through `DefaultOnResponse`.
#[allow(clippy::type_complexity)]
fn access_log_layer() -> tower_http::trace::TraceLayer<
    tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
    fn(&Request) -> tracing::Span,
    (),
    fn(&Response, std::time::Duration, &tracing::Span),
> {
    fn make_span(request: &Request) -> tracing::Span {
        tracing::info_span!(
            "http_request",
            method = %request.method(),
            path = %request.uri().path(),
            actor = tracing::field::Empty,
        )
    }
    fn on_response(response: &Response, latency: std::time::Duration, _span: &tracing::Span) {
        tracing::info!(
            status = response.status().as_u16(),
            duration_ms = latency.as_millis(),
            "http request"
        );
    }
    tower_http::trace::TraceLayer::new_for_http()
        .make_span_with(make_span as fn(&Request) -> _)
        // The request-side event is redundant with the completion event.
        .on_request(())
        .on_response(on_response as fn(&Response, std::time::Duration, &tracing::Span))
}

/// Construct state, run startup validation, and serve until ctrl-c.
pub async fn serve(config: Arc<Config>, client: Arc<PocketIdClient>) -> anyhow::Result<()> {
    let http_config = config
        .http
        .clone()
        .ok_or_else(|| anyhow::anyhow!("HTTP transport selected without HTTP configuration"))?;

    let state = match &http_config.auth {
        HttpAuthMode::OAuth(oauth_config) => {
            let authenticator = Authenticator::new(
                oauth_config.clone(),
                http_config.public_url.clone(),
                config.pocket_id_url.clone(),
                client.clone(),
            );
            let discovery = authenticator.init().await.map_err(|e| {
                anyhow::anyhow!(
                    "OAuth issuer validation failed for {}: {e}",
                    oauth_config.issuer
                )
            })?;
            tracing::info!(
                auth_mode = "oauth",
                issuer = %discovery.issuer,
                jwks = %discovery.jwks_uri,
                resource = %http_config.public_url,
                "OAuth resource server initialized"
            );
            HttpState::OAuth(Box::new(OAuthState {
                metadata_url: metadata_url_for(&http_config.public_url),
                resource: http_config.public_url.clone(),
                issuer: oauth_config.issuer.clone(),
                authenticator,
            }))
        }
        HttpAuthMode::StaticToken { token } => {
            tracing::info!(auth_mode = "token", "static bearer token authentication");
            HttpState::StaticToken {
                token: token.clone(),
            }
        }
        HttpAuthMode::None => {
            tracing::warn!(
                auth_mode = "none",
                bind = %http_config.bind,
                "serving WITHOUT authentication — anyone who can reach this port \
                 has full access to the configured Pocket ID API key's privileges"
            );
            HttpState::None
        }
    };

    let router = build_router(config.clone(), client, Arc::new(state));
    let listener = tokio::net::TcpListener::bind(&http_config.bind).await?;
    tracing::info!(
        bind = %http_config.bind,
        auth_mode = http_config.auth.name(),
        "serving MCP over streamable HTTP at /mcp"
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
