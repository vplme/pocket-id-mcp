//! OAuth 2.1 bearer-token validation: issuer discovery, JWKS caching,
//! JWT validation with audience binding, introspection fallback, and
//! group-based admission.

use std::sync::Arc;

use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::client::PocketIdClient;
use crate::config::HttpConfig;

#[derive(Debug)]
pub enum AuthError {
    /// Token missing, malformed, expired, wrong issuer, or wrong audience → 401.
    Unauthorized(String),
    /// Token valid but the caller is not admitted (group restriction) → 403.
    Forbidden(String),
    /// Upstream/issuer infrastructure failure → 502-ish, reported as 401 to
    /// avoid oracle behavior, logged server-side.
    Internal(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscoveryDocument {
    pub issuer: String,
    pub jwks_uri: String,
    #[serde(default)]
    pub introspection_endpoint: Option<String>,
    #[serde(default)]
    pub authorization_endpoint: Option<String>,
}

pub struct Authenticator {
    http_config: HttpConfig,
    pocket_id_url: String,
    pocket_client: Arc<PocketIdClient>,
    http: reqwest::Client,
    discovery: RwLock<Option<DiscoveryDocument>>,
    jwks: RwLock<Option<JwkSet>>,
}

impl Authenticator {
    pub fn new(
        http_config: HttpConfig,
        pocket_id_url: String,
        pocket_client: Arc<PocketIdClient>,
    ) -> Self {
        Self {
            http_config,
            pocket_id_url,
            pocket_client,
            http: reqwest::Client::builder()
                .user_agent(concat!("pocket-id-mcp/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("static reqwest config"),
            discovery: RwLock::new(None),
            jwks: RwLock::new(None),
        }
    }

    pub fn resource(&self) -> &str {
        &self.http_config.public_url
    }

    pub fn issuer(&self) -> &str {
        &self.http_config.oauth_issuer
    }

    /// Fetch (or return cached) issuer discovery metadata. Tries OIDC
    /// discovery first, then RFC 8414 OAuth metadata.
    pub async fn discovery(&self) -> Result<DiscoveryDocument, AuthError> {
        if let Some(doc) = self.discovery.read().await.clone() {
            return Ok(doc);
        }
        let issuer = self.http_config.oauth_issuer.trim_end_matches('/');
        let candidates = [
            format!("{issuer}/.well-known/openid-configuration"),
            format!("{issuer}/.well-known/oauth-authorization-server"),
        ];
        let mut last_err = String::new();
        for url in &candidates {
            match self.http.get(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<DiscoveryDocument>().await {
                        Ok(doc) => {
                            *self.discovery.write().await = Some(doc.clone());
                            return Ok(doc);
                        }
                        Err(e) => last_err = format!("{url}: invalid metadata: {e}"),
                    }
                }
                Ok(resp) => last_err = format!("{url}: HTTP {}", resp.status()),
                Err(e) => last_err = format!("{url}: {e}"),
            }
        }
        Err(AuthError::Internal(format!(
            "issuer discovery failed: {last_err}"
        )))
    }

    async fn fetch_jwks(&self) -> Result<JwkSet, AuthError> {
        let doc = self.discovery().await?;
        let jwks: JwkSet = self
            .http
            .get(&doc.jwks_uri)
            .send()
            .await
            .map_err(|e| AuthError::Internal(format!("JWKS fetch failed: {e}")))?
            .json()
            .await
            .map_err(|e| AuthError::Internal(format!("JWKS parse failed: {e}")))?;
        *self.jwks.write().await = Some(jwks.clone());
        Ok(jwks)
    }

    async fn key_for(&self, kid: Option<&str>) -> Result<Jwk, AuthError> {
        let pick = |set: &JwkSet| -> Option<Jwk> {
            match kid {
                Some(kid) => set.find(kid).cloned(),
                // No kid: usable only when the set has exactly one key.
                None => match set.keys.as_slice() {
                    [only] => Some(only.clone()),
                    _ => None,
                },
            }
        };
        if let Some(set) = self.jwks.read().await.as_ref() {
            if let Some(key) = pick(set) {
                return Ok(key);
            }
        }
        // Unknown kid: refresh once (handles issuer key rotation).
        let set = self.fetch_jwks().await?;
        pick(&set)
            .ok_or_else(|| AuthError::Unauthorized("token signed with unknown key".to_string()))
    }

    /// Startup validation: resolve discovery metadata and fetch JWKS.
    pub async fn init(&self) -> Result<DiscoveryDocument, String> {
        let doc = self.discovery().await.map_err(|e| format!("{e:?}"))?;
        self.fetch_jwks().await.map_err(|e| format!("{e:?}"))?;
        Ok(doc)
    }

    /// Validate a bearer token and return its claims.
    pub async fn validate(&self, token: &str) -> Result<serde_json::Value, AuthError> {
        let claims = match decode_header(token) {
            Ok(header) => self.validate_jwt(token, header).await?,
            // Not a JWS — opaque token: introspection fallback.
            Err(_) => self.introspect(token).await?,
        };
        self.check_groups(&claims)?;
        Ok(claims)
    }

    async fn validate_jwt(
        &self,
        token: &str,
        header: jsonwebtoken::Header,
    ) -> Result<serde_json::Value, AuthError> {
        let jwk = self.key_for(header.kid.as_deref()).await?;
        let key = DecodingKey::from_jwk(&jwk)
            .map_err(|e| AuthError::Internal(format!("unusable JWK: {e}")))?;
        let alg = jwk
            .common
            .key_algorithm
            .and_then(|a| a.to_string().parse::<Algorithm>().ok())
            .unwrap_or(header.alg);
        let mut validation = Validation::new(alg);
        validation.set_issuer(&[self.issuer()]);
        validation.set_audience(&[self.resource()]);
        validation.validate_exp = true;
        let data = decode::<serde_json::Value>(token, &key, &validation).map_err(|e| {
            use jsonwebtoken::errors::ErrorKind::*;
            let reason = match e.kind() {
                InvalidAudience => {
                    "token audience does not match this server's resource identifier"
                }
                InvalidIssuer => "token issuer mismatch",
                ExpiredSignature => "token expired",
                ImmatureSignature => "token not yet valid",
                InvalidSignature => "invalid token signature",
                _ => "token validation failed",
            };
            AuthError::Unauthorized(reason.to_string())
        })?;
        Ok(data.claims)
    }

    /// RFC 7662 fallback for opaque tokens. Only supported when the issuer is
    /// the Pocket ID instance itself, whose introspection endpoint accepts the
    /// server's API key; generic issuers require resource-server credentials
    /// this server deliberately does not hold.
    async fn introspect(&self, token: &str) -> Result<serde_json::Value, AuthError> {
        let issuer = self.issuer().trim_end_matches('/');
        if issuer != self.pocket_id_url.trim_end_matches('/') {
            return Err(AuthError::Unauthorized(
                "opaque tokens are not accepted from external issuers; present a JWT access token"
                    .to_string(),
            ));
        }
        let claims: serde_json::Value = self
            .pocket_client
            .form("/api/oidc/introspect", &[("token", token)])
            .await
            .map_err(|e| AuthError::Unauthorized(format!("introspection failed: {e}")))?;
        if claims.get("active").and_then(|a| a.as_bool()) != Some(true) {
            return Err(AuthError::Unauthorized("token is not active".to_string()));
        }
        // Audience binding: enforced when the introspection response carries an aud.
        if let Some(aud) = claims.get("aud") {
            let matches = match aud {
                serde_json::Value::String(s) => s == self.resource(),
                serde_json::Value::Array(arr) => {
                    arr.iter().any(|v| v.as_str() == Some(self.resource()))
                }
                _ => false,
            };
            if !matches {
                return Err(AuthError::Unauthorized(
                    "token audience does not match this server's resource identifier".to_string(),
                ));
            }
        }
        Ok(claims)
    }

    fn check_groups(&self, claims: &serde_json::Value) -> Result<(), AuthError> {
        let Some(allowed) = &self.http_config.allowed_groups else {
            return Ok(());
        };
        let claim_name = &self.http_config.groups_claim;
        let groups: Vec<String> = match claims.get(claim_name) {
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            Some(serde_json::Value::String(s)) => {
                s.split_whitespace().map(str::to_string).collect()
            }
            _ => Vec::new(),
        };
        if groups.iter().any(|g| allowed.contains(g)) {
            Ok(())
        } else {
            Err(AuthError::Forbidden(format!(
                "token's \"{claim_name}\" claim does not include any allowed group"
            )))
        }
    }
}
