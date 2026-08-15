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

#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// Socket address the HTTP server binds to.
    pub bind: String,
    /// Externally visible base URL — the OAuth resource identifier.
    pub public_url: String,
    /// OAuth authorization server issuer URL (defaults to the Pocket ID instance).
    pub oauth_issuer: String,
    /// When set, tokens must carry at least one of these groups.
    pub allowed_groups: Option<Vec<String>>,
    /// Claim name holding the group list.
    pub groups_claim: String,
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
            let public_url = validate_url(
                "POCKET_ID_MCP_PUBLIC_URL",
                &require(vars, "POCKET_ID_MCP_PUBLIC_URL")?,
            )?;
            let oauth_issuer = match vars.get("POCKET_ID_MCP_OAUTH_ISSUER") {
                Some(v) if !v.trim().is_empty() => {
                    validate_url("POCKET_ID_MCP_OAUTH_ISSUER", v.trim())?
                }
                _ => pocket_id_url.clone(),
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
            Some(HttpConfig {
                bind: vars
                    .get("POCKET_ID_MCP_HTTP_BIND")
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| "127.0.0.1:8756".to_string()),
                public_url,
                oauth_issuer,
                allowed_groups,
                groups_claim,
            })
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
        let mut vars = base_vars();
        vars.insert("POCKET_ID_MCP_TRANSPORT".into(), "http".into());
        let err = Config::from_vars(&vars).unwrap_err();
        assert!(err.to_string().contains("POCKET_ID_MCP_PUBLIC_URL"));
    }

    #[test]
    fn http_mode_defaults() {
        let mut vars = base_vars();
        vars.insert("POCKET_ID_MCP_TRANSPORT".into(), "http".into());
        vars.insert(
            "POCKET_ID_MCP_PUBLIC_URL".into(),
            "https://mcp.example.com".into(),
        );
        let cfg = Config::from_vars(&vars).unwrap();
        let http = cfg.http.unwrap();
        assert_eq!(http.bind, "127.0.0.1:8756");
        assert_eq!(http.public_url, "https://mcp.example.com");
        assert_eq!(http.oauth_issuer, "https://id.example.com");
        assert_eq!(http.groups_claim, "groups");
        assert!(http.allowed_groups.is_none());
    }

    #[test]
    fn http_mode_custom_issuer_and_groups() {
        let mut vars = base_vars();
        vars.insert("POCKET_ID_MCP_TRANSPORT".into(), "http".into());
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
        assert_eq!(
            http.oauth_issuer,
            "https://keycloak.example.com/realms/main"
        );
        assert_eq!(
            http.allowed_groups,
            Some(vec!["admins".to_string(), "ops".to_string()])
        );
        assert_eq!(http.groups_claim, "realm_roles");
    }
}
