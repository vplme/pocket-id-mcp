use std::collections::HashMap;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("invalid value for {var}: {reason}")]
    Invalid { var: &'static str, reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Stdio,
    Http,
}

/// OAuth-mode-only settings, present exactly when the auth mode is `oauth`.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    /// OAuth authorization server issuer URL (defaults to the Pocket ID instance).
    pub issuer: String,
    /// When set, tokens must carry at least one of these groups.
    pub allowed_groups: Option<Vec<String>>,
    /// Claim name holding the group list.
    pub groups_claim: String,
}

/// How the HTTP transport authenticates callers.
#[derive(Debug, Clone)]
pub enum HttpAuthMode {
    /// OAuth 2.1 protected resource (the default).
    OAuth(OAuthConfig),
    /// Static shared bearer secret, compared in constant time.
    StaticToken { token: String },
    /// No authentication; only permitted on a loopback bind unless overridden.
    None,
}

impl HttpAuthMode {
    /// Short name as accepted by `POCKET_ID_MCP_HTTP_AUTH`, for logging.
    pub fn name(&self) -> &'static str {
        match self {
            HttpAuthMode::OAuth(_) => "oauth",
            HttpAuthMode::StaticToken { .. } => "token",
            HttpAuthMode::None => "none",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// Socket address the HTTP server binds to.
    pub bind: String,
    /// Externally visible base URL; in `oauth` mode also the resource identifier.
    pub public_url: String,
    pub auth: HttpAuthMode,
}

fn lenient_bool(value: Option<&String>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("true") | Some("1") | Some("yes")
    )
}

fn require(vars: &HashMap<String, String>, name: &'static str) -> Result<String, ConfigError> {
    match vars.get(name).map(|v| v.trim().to_string()) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => Err(ConfigError::Missing(name)),
    }
}

fn validate_url(var: &'static str, value: &str) -> Result<String, ConfigError> {
    let parsed = url::Url::parse(value).map_err(|e| ConfigError::Invalid {
        var,
        reason: e.to_string(),
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ConfigError::Invalid {
            var,
            reason: format!("unsupported scheme {}", parsed.scheme()),
        });
    }
    Ok(value.trim_end_matches('/').to_string())
}

fn is_set(vars: &HashMap<String, String>, name: &str) -> bool {
    vars.get(name).is_some_and(|v| !v.trim().is_empty())
}

fn reject_if_set(
    vars: &HashMap<String, String>,
    name: &'static str,
    mode: &str,
) -> Result<(), ConfigError> {
    if is_set(vars, name) {
        Err(ConfigError::Invalid {
            var: name,
            reason: format!("not allowed when POCKET_ID_MCP_HTTP_AUTH={mode}"),
        })
    } else {
        Ok(())
    }
}

/// Whether a `host:port` bind address targets a loopback interface
/// (`127.0.0.0/8` literal, `::1`, or `localhost`).
fn is_loopback_bind(bind: &str) -> bool {
    let host = match bind.rsplit_once(':') {
        Some((h, _)) => h.trim_start_matches('[').trim_end_matches(']'),
        None => bind,
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Base URL when `POCKET_ID_MCP_PUBLIC_URL` is omitted in non-OAuth modes.
fn default_public_url(bind: &str) -> String {
    let port = bind.rsplit_once(':').map(|(_, p)| p).unwrap_or("8756");
    format!("http://localhost:{port}")
}

#[derive(Debug, Clone)]
pub struct Config {
    pub pocket_id_url: String,
    pub api_key: String,
    pub read_only: bool,
    pub allow_dangerous: bool,
    pub transport: Transport,
    pub http: Option<HttpConfig>,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let vars: HashMap<String, String> = std::env::vars().collect();
        Self::from_vars(&vars)
    }

    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let pocket_id_url = validate_url("POCKET_ID_URL", &require(vars, "POCKET_ID_URL")?)?;
        let api_key = require(vars, "POCKET_ID_API_KEY")?;
        let read_only = lenient_bool(vars.get("POCKET_ID_MCP_READ_ONLY"));
        let allow_dangerous = lenient_bool(vars.get("POCKET_ID_MCP_ALLOW_DANGEROUS"));

        let transport = match vars
            .get("POCKET_ID_MCP_TRANSPORT")
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref()
        {
            None | Some("") | Some("stdio") => Transport::Stdio,
            Some("http") => Transport::Http,
            Some(other) => {
                return Err(ConfigError::Invalid {
                    var: "POCKET_ID_MCP_TRANSPORT",
                    reason: format!("expected \"stdio\" or \"http\", got {other:?}"),
                });
            }
        };

        let http = if transport == Transport::Http {
            Some(Self::http_from_vars(vars, &pocket_id_url)?)
        } else {
            None
        };

        Ok(Config {
            pocket_id_url,
            api_key,
            read_only,
            allow_dangerous,
            transport,
            http,
        })
    }

    fn http_from_vars(
        vars: &HashMap<String, String>,
        pocket_id_url: &str,
    ) -> Result<HttpConfig, ConfigError> {
        let bind = vars
            .get("POCKET_ID_MCP_HTTP_BIND")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "127.0.0.1:8756".to_string());

        let mode = vars
            .get("POCKET_ID_MCP_HTTP_AUTH")
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "oauth".to_string());

        // Optional outside oauth mode: it only exists as the OAuth resource
        // identifier, so local setups shouldn't have to invent one.
        let public_url = match vars.get("POCKET_ID_MCP_PUBLIC_URL") {
            Some(v) if !v.trim().is_empty() => validate_url("POCKET_ID_MCP_PUBLIC_URL", v.trim())?,
            _ if mode == "oauth" => return Err(ConfigError::Missing("POCKET_ID_MCP_PUBLIC_URL")),
            _ => default_public_url(&bind),
        };

        let auth = match mode.as_str() {
            "oauth" => {
                reject_if_set(vars, "POCKET_ID_MCP_HTTP_TOKEN", "oauth")?;
                let issuer = match vars.get("POCKET_ID_MCP_OAUTH_ISSUER") {
                    Some(v) if !v.trim().is_empty() => {
                        validate_url("POCKET_ID_MCP_OAUTH_ISSUER", v.trim())?
                    }
                    _ => pocket_id_url.to_string(),
                };
                let allowed_groups = vars.get("POCKET_ID_MCP_ALLOWED_GROUPS").and_then(|v| {
                    let groups: Vec<String> = v
                        .split(',')
                        .map(|g| g.trim().to_string())
                        .filter(|g| !g.is_empty())
                        .collect();
                    if groups.is_empty() {
                        None
                    } else {
                        Some(groups)
                    }
                });
                let groups_claim = vars
                    .get("POCKET_ID_MCP_GROUPS_CLAIM")
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| "groups".to_string());
                HttpAuthMode::OAuth(OAuthConfig {
                    issuer,
                    allowed_groups,
                    groups_claim,
                })
            }
            "token" => {
                // Rejected (not ignored) so nobody believes group admission or
                // a custom issuer is in effect when it is not.
                reject_if_set(vars, "POCKET_ID_MCP_OAUTH_ISSUER", "token")?;
                reject_if_set(vars, "POCKET_ID_MCP_ALLOWED_GROUPS", "token")?;
                reject_if_set(vars, "POCKET_ID_MCP_GROUPS_CLAIM", "token")?;
                HttpAuthMode::StaticToken {
                    token: require(vars, "POCKET_ID_MCP_HTTP_TOKEN")?,
                }
            }
            "none" => {
                reject_if_set(vars, "POCKET_ID_MCP_OAUTH_ISSUER", "none")?;
                reject_if_set(vars, "POCKET_ID_MCP_ALLOWED_GROUPS", "none")?;
                reject_if_set(vars, "POCKET_ID_MCP_GROUPS_CLAIM", "none")?;
                reject_if_set(vars, "POCKET_ID_MCP_HTTP_TOKEN", "none")?;
                // This server fronts an admin API key: an unauthenticated
                // non-loopback bind hands full IdP admin to the network.
                if !is_loopback_bind(&bind)
                    && !lenient_bool(
                        vars.get("POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK"),
                    )
                {
                    return Err(ConfigError::Invalid {
                        var: "POCKET_ID_MCP_HTTP_BIND",
                        reason: format!(
                            "unauthenticated mode requires a loopback bind (got {bind:?}); \
                             set POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK=true \
                             to override"
                        ),
                    });
                }
                HttpAuthMode::None
            }
            other => {
                return Err(ConfigError::Invalid {
                    var: "POCKET_ID_MCP_HTTP_AUTH",
                    reason: format!("expected \"oauth\", \"token\", or \"none\", got {other:?}"),
                });
            }
        };

        Ok(HttpConfig {
            bind,
            public_url,
            auth,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_vars() -> HashMap<String, String> {
        HashMap::from([
            ("POCKET_ID_URL".into(), "https://id.example.com".into()),
            ("POCKET_ID_API_KEY".into(), "test-key".into()),
        ])
    }

    fn http_vars() -> HashMap<String, String> {
        let mut vars = base_vars();
        vars.insert("POCKET_ID_MCP_TRANSPORT".into(), "http".into());
        vars
    }

    fn oauth(http: &HttpConfig) -> &OAuthConfig {
        match &http.auth {
            HttpAuthMode::OAuth(o) => o,
            other => panic!("expected oauth mode, got {other:?}"),
        }
    }

    #[test]
    fn minimal_config_defaults() {
        let cfg = Config::from_vars(&base_vars()).unwrap();
        assert_eq!(cfg.pocket_id_url, "https://id.example.com");
        assert_eq!(cfg.api_key, "test-key");
        assert!(!cfg.read_only);
        assert!(!cfg.allow_dangerous);
        assert_eq!(cfg.transport, Transport::Stdio);
        assert!(cfg.http.is_none());
    }

    #[test]
    fn missing_url_named_in_error() {
        let mut vars = base_vars();
        vars.remove("POCKET_ID_URL");
        let err = Config::from_vars(&vars).unwrap_err();
        assert!(err.to_string().contains("POCKET_ID_URL"));
    }

    #[test]
    fn missing_api_key_named_in_error() {
        let mut vars = base_vars();
        vars.remove("POCKET_ID_API_KEY");
        let err = Config::from_vars(&vars).unwrap_err();
        assert!(err.to_string().contains("POCKET_ID_API_KEY"));
    }

    #[test]
    fn trailing_slash_stripped() {
        let mut vars = base_vars();
        vars.insert("POCKET_ID_URL".into(), "https://id.example.com/".into());
        let cfg = Config::from_vars(&vars).unwrap();
        assert_eq!(cfg.pocket_id_url, "https://id.example.com");
    }

    #[test]
    fn lenient_booleans() {
        for (value, expected) in [
            ("true", true),
            ("TRUE", true),
            ("1", true),
            ("yes", true),
            ("Yes", true),
            ("false", false),
            ("0", false),
            ("no", false),
            ("banana", false),
            ("", false),
        ] {
            let mut vars = base_vars();
            vars.insert("POCKET_ID_MCP_READ_ONLY".into(), value.into());
            vars.insert("POCKET_ID_MCP_ALLOW_DANGEROUS".into(), value.into());
            let cfg = Config::from_vars(&vars).unwrap();
            assert_eq!(cfg.read_only, expected, "value {value:?}");
            assert_eq!(cfg.allow_dangerous, expected, "value {value:?}");
        }
    }

    #[test]
    fn invalid_transport_rejected() {
        let mut vars = base_vars();
        vars.insert("POCKET_ID_MCP_TRANSPORT".into(), "websocket".into());
        let err = Config::from_vars(&vars).unwrap_err();
        assert!(err.to_string().contains("POCKET_ID_MCP_TRANSPORT"));
    }

    #[test]
    fn http_mode_requires_public_url() {
        let vars = http_vars();
        let err = Config::from_vars(&vars).unwrap_err();
        assert!(err.to_string().contains("POCKET_ID_MCP_PUBLIC_URL"));
    }

    #[test]
    fn http_mode_defaults() {
        let mut vars = http_vars();
        vars.insert(
            "POCKET_ID_MCP_PUBLIC_URL".into(),
            "https://mcp.example.com".into(),
        );
        let cfg = Config::from_vars(&vars).unwrap();
        let http = cfg.http.unwrap();
        assert_eq!(http.bind, "127.0.0.1:8756");
        assert_eq!(http.public_url, "https://mcp.example.com");
        let oauth = oauth(&http);
        assert_eq!(oauth.issuer, "https://id.example.com");
        assert_eq!(oauth.groups_claim, "groups");
        assert!(oauth.allowed_groups.is_none());
    }

    #[test]
    fn http_mode_custom_issuer_and_groups() {
        let mut vars = http_vars();
        vars.insert(
            "POCKET_ID_MCP_PUBLIC_URL".into(),
            "https://mcp.example.com".into(),
        );
        vars.insert(
            "POCKET_ID_MCP_OAUTH_ISSUER".into(),
            "https://keycloak.example.com/realms/main".into(),
        );
        vars.insert("POCKET_ID_MCP_ALLOWED_GROUPS".into(), "admins, ops,".into());
        vars.insert("POCKET_ID_MCP_GROUPS_CLAIM".into(), "realm_roles".into());
        let cfg = Config::from_vars(&vars).unwrap();
        let http = cfg.http.unwrap();
        let oauth = oauth(&http);
        assert_eq!(oauth.issuer, "https://keycloak.example.com/realms/main");
        assert_eq!(
            oauth.allowed_groups,
            Some(vec!["admins".to_string(), "ops".to_string()])
        );
        assert_eq!(oauth.groups_claim, "realm_roles");
    }

    #[test]
    fn invalid_auth_mode_rejected() {
        let mut vars = http_vars();
        vars.insert("POCKET_ID_MCP_HTTP_AUTH".into(), "mtls".into());
        let err = Config::from_vars(&vars).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("POCKET_ID_MCP_HTTP_AUTH"), "got: {msg}");
        assert!(msg.contains("oauth"), "got: {msg}");
    }

    #[test]
    fn static_token_in_oauth_mode_rejected() {
        let mut vars = http_vars();
        vars.insert(
            "POCKET_ID_MCP_PUBLIC_URL".into(),
            "https://mcp.example.com".into(),
        );
        vars.insert("POCKET_ID_MCP_HTTP_TOKEN".into(), "secret".into());
        let err = Config::from_vars(&vars).unwrap_err();
        assert!(err.to_string().contains("POCKET_ID_MCP_HTTP_TOKEN"));
    }

    #[test]
    fn token_mode_parsed_with_defaulted_public_url() {
        let mut vars = http_vars();
        vars.insert("POCKET_ID_MCP_HTTP_AUTH".into(), "token".into());
        vars.insert("POCKET_ID_MCP_HTTP_TOKEN".into(), "s3cret".into());
        let cfg = Config::from_vars(&vars).unwrap();
        let http = cfg.http.unwrap();
        assert_eq!(http.public_url, "http://localhost:8756");
        match &http.auth {
            HttpAuthMode::StaticToken { token } => assert_eq!(token, "s3cret"),
            other => panic!("expected token mode, got {other:?}"),
        }
    }

    #[test]
    fn token_mode_requires_token() {
        let mut vars = http_vars();
        vars.insert("POCKET_ID_MCP_HTTP_AUTH".into(), "token".into());
        let err = Config::from_vars(&vars).unwrap_err();
        assert!(err.to_string().contains("POCKET_ID_MCP_HTTP_TOKEN"));
    }

    #[test]
    fn oauth_only_vars_rejected_outside_oauth_mode() {
        for mode in ["token", "none"] {
            for var in [
                "POCKET_ID_MCP_OAUTH_ISSUER",
                "POCKET_ID_MCP_ALLOWED_GROUPS",
                "POCKET_ID_MCP_GROUPS_CLAIM",
            ] {
                let mut vars = http_vars();
                vars.insert("POCKET_ID_MCP_HTTP_AUTH".into(), mode.into());
                if mode == "token" {
                    vars.insert("POCKET_ID_MCP_HTTP_TOKEN".into(), "secret".into());
                }
                vars.insert(var.into(), "https://id.example.com".into());
                let err = Config::from_vars(&vars).unwrap_err();
                let msg = err.to_string();
                assert!(msg.contains(var), "mode {mode}, var {var}: {msg}");
            }
        }
    }

    #[test]
    fn none_mode_on_loopback_allowed() {
        for bind in [
            "127.0.0.1:9000",
            "127.1.2.3:9000",
            "[::1]:9000",
            "localhost:9000",
        ] {
            let mut vars = http_vars();
            vars.insert("POCKET_ID_MCP_HTTP_AUTH".into(), "none".into());
            vars.insert("POCKET_ID_MCP_HTTP_BIND".into(), bind.into());
            let cfg = Config::from_vars(&vars).unwrap();
            let http = cfg.http.unwrap();
            assert!(matches!(http.auth, HttpAuthMode::None), "bind {bind}");
            assert_eq!(http.public_url, "http://localhost:9000", "bind {bind}");
        }
    }

    #[test]
    fn none_mode_on_non_loopback_refused() {
        let mut vars = http_vars();
        vars.insert("POCKET_ID_MCP_HTTP_AUTH".into(), "none".into());
        vars.insert("POCKET_ID_MCP_HTTP_BIND".into(), "0.0.0.0:8756".into());
        let err = Config::from_vars(&vars).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("loopback"), "got: {msg}");
        assert!(
            msg.contains("POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK"),
            "got: {msg}"
        );
    }

    #[test]
    fn none_mode_non_loopback_override_honored() {
        let mut vars = http_vars();
        vars.insert("POCKET_ID_MCP_HTTP_AUTH".into(), "none".into());
        vars.insert("POCKET_ID_MCP_HTTP_BIND".into(), "0.0.0.0:8756".into());
        vars.insert(
            "POCKET_ID_MCP_HTTP_ALLOW_UNAUTHENTICATED_NON_LOOPBACK".into(),
            "true".into(),
        );
        let cfg = Config::from_vars(&vars).unwrap();
        assert!(matches!(cfg.http.unwrap().auth, HttpAuthMode::None));
    }

    #[test]
    fn explicit_public_url_kept_in_none_mode() {
        let mut vars = http_vars();
        vars.insert("POCKET_ID_MCP_HTTP_AUTH".into(), "none".into());
        vars.insert(
            "POCKET_ID_MCP_PUBLIC_URL".into(),
            "http://mcp.internal:8756".into(),
        );
        let cfg = Config::from_vars(&vars).unwrap();
        assert_eq!(cfg.http.unwrap().public_url, "http://mcp.internal:8756");
    }
}
