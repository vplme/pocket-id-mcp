//! OAuth 2.1 bearer-token validation: issuer discovery, JWKS caching,
//! JWT validation with audience binding, and group-based admission.

use jsonwebtoken::jwk::{AlgorithmParameters, EllipticCurve, Jwk, JwkSet};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::config::OAuthConfig;

/// Minimum age of the cached JWKS before an unknown-kid token may trigger a
/// refetch. Without it, every request carrying an unknown kid causes an
/// upstream fetch, letting unauthenticated callers use this server as
/// request amplification against the IdP. Key rotation still converges
/// within this window.
const JWKS_REFRESH_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(60);

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

struct CachedJwks {
    keys: JwkSet,
    fetched_at: std::time::Instant,
}

pub struct Authenticator {
    oauth: OAuthConfig,
    /// OAuth resource identifier (the public URL) tokens must be bound to.
    resource: String,
    http: reqwest::Client,
    discovery: RwLock<Option<DiscoveryDocument>>,
    jwks: RwLock<Option<CachedJwks>>,
}

impl Authenticator {
    pub fn new(oauth: OAuthConfig, resource: String) -> Self {
        Self {
            oauth,
            resource,
            http: reqwest::Client::builder()
                .user_agent(concat!("pocket-id-mcp/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(crate::client::CONNECT_TIMEOUT)
                .timeout(crate::client::REQUEST_TIMEOUT)
                .build()
                .expect("static reqwest config"),
            discovery: RwLock::new(None),
            jwks: RwLock::new(None),
        }
    }

    pub fn resource(&self) -> &str {
        &self.resource
    }

    pub fn issuer(&self) -> &str {
        &self.oauth.issuer
    }

    /// Fetch (or return cached) issuer discovery metadata. Tries OIDC
    /// discovery first, then RFC 8414 OAuth metadata.
    pub async fn discovery(&self) -> Result<DiscoveryDocument, AuthError> {
        if let Some(doc) = self.discovery.read().await.clone() {
            return Ok(doc);
        }
        let issuer = self.oauth.issuer.trim_end_matches('/');
        let candidates = [
            format!("{issuer}/.well-known/openid-configuration"),
            format!("{issuer}/.well-known/oauth-authorization-server"),
        ];
        let mut last_err = String::new();
        for url in &candidates {
            match self.http.get(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<DiscoveryDocument>().await {
                        // RFC 8414 §3.3: the metadata's issuer must match the
                        // configured one, or tokens minted under the declared
                        // issuer would fail every iss check while startup
                        // succeeds, masking the misconfiguration. Compared
                        // slash-insensitively, like the iss claim itself.
                        Ok(doc) if doc.issuer.trim_end_matches('/') != issuer => {
                            last_err = format!(
                                "{url}: metadata declares issuer {:?}, expected {:?}",
                                doc.issuer, self.oauth.issuer
                            );
                        }
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
        // The write lock is held across the fetch on purpose: concurrent
        // unknown-kid requests queue here instead of each firing their own
        // upstream fetch, and the fresh-cache re-check below turns the
        // queued ones into cache hits.
        let mut slot = self.jwks.write().await;
        if let Some(cache) = slot.as_ref() {
            if cache.fetched_at.elapsed() < JWKS_REFRESH_COOLDOWN {
                return Ok(cache.keys.clone());
            }
        }
        let jwks: JwkSet = self
            .http
            .get(&doc.jwks_uri)
            .send()
            .await
            .map_err(|e| AuthError::Internal(format!("JWKS fetch failed: {e}")))?
            .json()
            .await
            .map_err(|e| AuthError::Internal(format!("JWKS parse failed: {e}")))?;
        *slot = Some(CachedJwks {
            keys: jwks.clone(),
            fetched_at: std::time::Instant::now(),
        });
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
        if let Some(cache) = self.jwks.read().await.as_ref() {
            if let Some(key) = pick(&cache.keys) {
                return Ok(key);
            }
            // Unknown kid against a recently fetched set is an invalid
            // token, not a rotation signal: refetching for every such token
            // would let unauthenticated callers drive upstream traffic.
            if cache.fetched_at.elapsed() < JWKS_REFRESH_COOLDOWN {
                return Err(AuthError::Unauthorized(
                    "token signed with unknown key".to_string(),
                ));
            }
        }
        // Unknown kid and a cold or stale cache: refresh (handles issuer key
        // rotation), rate-limited by the cooldown.
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
            // Not a JWS — opaque token. RFC 7662 introspection is not an
            // option: Pocket ID's introspection endpoint requires OAuth
            // client credentials and only lets a client introspect its own
            // tokens, and generic issuers likewise require resource-server
            // credentials this server deliberately does not hold.
            Err(_) => {
                return Err(AuthError::Unauthorized(
                    "opaque tokens are not supported; present a JWT access token".to_string(),
                ));
            }
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
        let algorithms = allowed_algorithms(&jwk)?;
        let mut validation = Validation::new(algorithms[0]);
        validation.algorithms = algorithms;
        // The configured issuer is normalized without a trailing slash, but
        // some authorization servers' canonical iss ends in one (common for
        // path-based issuers like https://sts.example.com/tenant/). The two
        // spellings name the same issuer; accept both rather than failing
        // every token over a slash.
        let issuer = self.issuer().trim_end_matches('/');
        validation.set_issuer(&[issuer.to_string(), format!("{issuer}/")]);
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

    fn check_groups(&self, claims: &serde_json::Value) -> Result<(), AuthError> {
        let Some(allowed) = &self.oauth.allowed_groups else {
            return Ok(());
        };
        let claim_name = &self.oauth.groups_claim;
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

/// Verification algorithms admissible for a JWK, decided entirely server-side.
///
/// A JWK declaring `alg` pins exactly that algorithm. One without `alg` gets
/// the signature algorithms its key type supports. The token header's `alg`
/// is never consulted: it is attacker-controlled, and letting it pick the
/// algorithm invites downgrade and key-confusion attacks (e.g. HS256 keyed
/// with the public key material).
fn allowed_algorithms(jwk: &Jwk) -> Result<Vec<Algorithm>, AuthError> {
    if let Some(key_alg) = jwk.common.key_algorithm {
        return match key_alg.to_string().parse::<Algorithm>() {
            Ok(alg) => Ok(vec![alg]),
            // e.g. an encryption algorithm such as RSA-OAEP: not a key this
            // server may verify signatures against.
            Err(_) => Err(AuthError::Unauthorized(format!(
                "token signed with a key whose JWK declares the non-signature \
                 algorithm {key_alg}"
            ))),
        };
    }
    match &jwk.algorithm {
        AlgorithmParameters::RSA(_) => Ok(vec![
            Algorithm::RS256,
            Algorithm::RS384,
            Algorithm::RS512,
            Algorithm::PS256,
            Algorithm::PS384,
            Algorithm::PS512,
        ]),
        AlgorithmParameters::EllipticCurve(params) => match params.curve {
            EllipticCurve::P256 => Ok(vec![Algorithm::ES256]),
            EllipticCurve::P384 => Ok(vec![Algorithm::ES384]),
            ref other => Err(AuthError::Unauthorized(format!(
                "token signed with a key on the unsupported curve {other:?}"
            ))),
        },
        AlgorithmParameters::OctetKeyPair(_) => Ok(vec![Algorithm::EdDSA]),
        // A symmetric key in a public JWKS is an issuer misconfiguration;
        // verifying against it would make the "secret" public.
        AlgorithmParameters::OctetKey(_) => Err(AuthError::Internal(
            "issuer JWKS contains a symmetric key; refusing to verify against it".to_string(),
        )),
    }
}
