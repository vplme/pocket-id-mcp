//! MCP tool surface: tier model, coverage catalog, and shared helpers.

pub mod admin;
pub mod identity;
pub mod oidc;

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

/// Safety tier of a tool. Tools are registered only when their tier is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Always registered.
    Read,
    /// Registered unless `POCKET_ID_MCP_READ_ONLY` is enabled.
    Write,
    /// Registered only when `POCKET_ID_MCP_ALLOW_DANGEROUS` is enabled
    /// (and read-only mode is off).
    Dangerous,
}

impl Tier {
    /// Lowercase name, as it appears in log records.
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Read => "read",
            Tier::Write => "write",
            Tier::Dangerous => "dangerous",
        }
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Tier of a tool by name, from the same catalog that drives registration.
///
/// Returns `None` only for a name absent from [`CATALOG`], which the coverage
/// test makes impossible for a registered tool; callers log it as unknown
/// rather than treating it as an error.
pub fn tier_for(name: &str) -> Option<Tier> {
    CATALOG.iter().find(|t| t.name == name).map(|t| t.tier)
}

/// One tool with the swagger operations it covers. This drives the spec
/// coverage test — every swagger operation must appear here or in
/// `spec/exclusions.toml`.
pub struct ToolSpec {
    pub name: &'static str,
    pub tier: Tier,
    /// (HTTP method, swagger path) pairs this tool covers.
    pub operations: &'static [(&'static str, &'static str)],
}

macro_rules! catalog {
    ($( $name:literal / $tier:ident => [ $( $method:literal $path:literal ),+ $(,)? ] );+ $(;)?) => {
        &[ $( ToolSpec {
            name: $name,
            tier: Tier::$tier,
            operations: &[ $( ($method, $path) ),+ ],
        } ),+ ]
    };
}

/// The full tool catalog: name, tier, and covered swagger operations.
pub const CATALOG: &[ToolSpec] = catalog! {
    // --- identity: users -------------------------------------------------
    "list_users" / Read => ["GET" "/api/users"];
    "get_user" / Read => ["GET" "/api/users/{id}"];
    "get_current_user" / Read => ["GET" "/api/users/me"];
    "list_user_groups_of_user" / Read => ["GET" "/api/users/{id}/groups"];
    "get_user_profile_picture" / Read => ["GET" "/api/users/{id}/profile-picture.png"];
    "create_user" / Write => ["POST" "/api/users"];
    "update_user" / Write => ["PUT" "/api/users/{id}"];
    "update_current_user" / Write => ["PUT" "/api/users/me"];
    "delete_user" / Dangerous => ["DELETE" "/api/users/{id}"];
    "update_user_profile_picture" / Write => ["PUT" "/api/users/{id}/profile-picture"];
    "reset_user_profile_picture" / Write => ["DELETE" "/api/users/{id}/profile-picture"];
    "update_current_user_profile_picture" / Write => ["PUT" "/api/users/me/profile-picture"];
    "reset_current_user_profile_picture" / Write => ["DELETE" "/api/users/me/profile-picture"];
    "send_current_user_email_verification" / Write => ["POST" "/api/users/me/send-email-verification"];
    "verify_current_user_email" / Write => ["POST" "/api/users/me/verify-email"];
    "set_user_groups" / Write => ["PUT" "/api/users/{id}/user-groups"];
    // --- identity: groups ------------------------------------------------
    "list_groups" / Read => ["GET" "/api/user-groups"];
    "get_group" / Read => ["GET" "/api/user-groups/{id}"];
    "create_group" / Write => ["POST" "/api/user-groups"];
    "update_group" / Write => ["PUT" "/api/user-groups/{id}"];
    "delete_group" / Write => ["DELETE" "/api/user-groups/{id}"];
    "set_group_users" / Write => ["PUT" "/api/user-groups/{id}/users"];
    // --- identity: custom claims ----------------------------------------
    "get_custom_claim_suggestions" / Read => ["GET" "/api/custom-claims/suggestions"];
    "update_user_custom_claims" / Write => ["PUT" "/api/custom-claims/user/{userId}"];
    "update_group_custom_claims" / Write => ["PUT" "/api/custom-claims/user-group/{userGroupId}"];
    // --- identity: passkeys ----------------------------------------------
    "list_user_passkeys" / Read => ["GET" "/api/users/{id}/webauthn-credentials"];
    "delete_user_passkey" / Dangerous => ["DELETE" "/api/users/{id}/webauthn-credentials/{credentialId}"];
    // --- identity: signup and one-time access ----------------------------
    "list_signup_tokens" / Read => ["GET" "/api/signup-tokens"];
    "create_signup_token" / Dangerous => ["POST" "/api/signup-tokens"];
    "delete_signup_token" / Dangerous => ["DELETE" "/api/signup-tokens/{id}"];
    "create_one_time_access_token" / Dangerous => ["POST" "/api/users/{id}/one-time-access-token"];
    "send_one_time_access_email" / Dangerous => ["POST" "/api/users/{id}/one-time-access-email"];
    "request_one_time_access_email" / Dangerous => ["POST" "/api/one-time-access-email"];
    // --- oidc: clients ---------------------------------------------------
    "list_oidc_clients" / Read => ["GET" "/api/oidc/clients"];
    "get_oidc_client" / Read => ["GET" "/api/oidc/clients/{id}"];
    "create_oidc_client" / Write => ["POST" "/api/oidc/clients"];
    "update_oidc_client" / Write => ["PUT" "/api/oidc/clients/{id}"];
    "delete_oidc_client" / Write => ["DELETE" "/api/oidc/clients/{id}"];
    "create_oidc_client_secret" / Write => ["POST" "/api/oidc/clients/{id}/secret"];
    "update_oidc_client_allowed_groups" / Write => ["PUT" "/api/oidc/clients/{id}/allowed-user-groups"];
    "get_oidc_client_metadata" / Read => ["GET" "/api/oidc/clients/{id}/meta"];
    "refresh_oidc_client_metadata" / Write => ["POST" "/api/oidc/clients/{id}/refresh"];
    "preview_oidc_client_for_user" / Read => ["GET" "/api/oidc/clients/{id}/preview/{userId}"];
    "get_oidc_client_logo" / Read => ["GET" "/api/oidc/clients/{id}/logo"];
    "update_oidc_client_logo" / Write => ["POST" "/api/oidc/clients/{id}/logo"];
    "delete_oidc_client_logo" / Write => ["DELETE" "/api/oidc/clients/{id}/logo"];
    "set_group_allowed_oidc_clients" / Write => ["PUT" "/api/user-groups/{id}/allowed-oidc-clients"];
    // --- oidc: tokens and grants -----------------------------------------
    "introspect_token" / Read => ["POST" "/api/oidc/introspect"];
    "list_user_authorized_clients" / Read => ["GET" "/api/oidc/users/{id}/authorized-clients"];
    "list_my_authorized_clients" / Read => ["GET" "/api/oidc/users/me/authorized-clients"];
    "revoke_my_authorized_client" / Write => ["DELETE" "/api/oidc/users/me/authorized-clients/{clientId}"];
    "list_my_accessible_clients" / Read => ["GET" "/api/oidc/users/me/clients"];
    // --- oidc: API definitions and access --------------------------------
    "get_client_api_access" / Read => ["GET" "/api/api-access/{clientId}"];
    "update_client_api_access" / Write => ["PUT" "/api/api-access/{clientId}"];
    "list_api_definitions" / Read => ["GET" "/api/apis"];
    "get_api_definition" / Read => ["GET" "/api/apis/{id}"];
    "create_api_definition" / Write => ["POST" "/api/apis"];
    "update_api_definition" / Write => ["PUT" "/api/apis/{id}"];
    "delete_api_definition" / Write => ["DELETE" "/api/apis/{id}"];
    "set_api_definition_permissions" / Write => ["PUT" "/api/apis/{id}/permissions"];
    // --- admin: application images ---------------------------------------
    "get_application_image" / Read => [
        "GET" "/api/application-images/logo",
        "GET" "/api/application-images/favicon",
        "GET" "/api/application-images/background",
        "GET" "/api/application-images/email",
        "GET" "/api/application-images/default-profile-picture",
    ];
    "update_application_image" / Write => [
        "PUT" "/api/application-images/logo",
        "PUT" "/api/application-images/favicon",
        "PUT" "/api/application-images/background",
        "PUT" "/api/application-images/email",
        "PUT" "/api/application-images/default-profile-picture",
    ];
    "delete_application_image" / Write => [
        "DELETE" "/api/application-images/background",
        "DELETE" "/api/application-images/default-profile-picture",
    ];
    // --- admin: configuration --------------------------------------------
    "get_public_application_configuration" / Read => ["GET" "/api/application-configuration"];
    "get_all_application_configuration" / Read => ["GET" "/api/application-configuration/all"];
    "update_application_configuration" / Write => ["PUT" "/api/application-configuration"];
    "sync_ldap" / Write => ["POST" "/api/application-configuration/sync-ldap"];
    "send_test_email" / Write => ["POST" "/api/application-configuration/test-email"];
    // --- admin: audit logs -----------------------------------------------
    "list_my_audit_logs" / Read => ["GET" "/api/audit-logs"];
    "list_all_audit_logs" / Read => ["GET" "/api/audit-logs/all"];
    "list_audit_log_client_names" / Read => ["GET" "/api/audit-logs/filters/client-names"];
    "list_audit_log_users" / Read => ["GET" "/api/audit-logs/filters/users"];
    // --- admin: API keys --------------------------------------------------
    "list_api_keys" / Read => ["GET" "/api/api-keys"];
    "create_api_key" / Write => ["POST" "/api/api-keys"];
    "renew_api_key" / Write => ["POST" "/api/api-keys/{id}/renew"];
    "revoke_api_key" / Dangerous => ["DELETE" "/api/api-keys/{id}"];
    // --- admin: SCIM -------------------------------------------------------
    "create_scim_service_provider" / Write => ["POST" "/api/scim/service-provider"];
    "update_scim_service_provider" / Write => ["PUT" "/api/scim/service-provider/{id}"];
    "delete_scim_service_provider" / Write => ["DELETE" "/api/scim/service-provider/{id}"];
    "sync_scim_service_provider" / Write => ["POST" "/api/scim/service-provider/{id}/sync"];
    "get_client_scim_service_provider" / Read => ["GET" "/api/oidc/clients/{id}/scim-service-provider"];
    // --- admin: status -----------------------------------------------------
    "get_current_version" / Read => ["GET" "/api/version/current"];
    "get_latest_version" / Read => ["GET" "/api/version/latest"];
    "health_check" / Read => ["GET" "/healthz"];
};

/// Request parameters that may appear in log records.
///
/// An allowlist, deliberately not a denylist: the safe and unsafe names here
/// are lexically adjacent — `token_id` identifies a signup token while `token`
/// (`introspect_token`) is a live bearer token, and `key_id` identifies an API
/// key while `key` is the key material. Any pattern broad enough to catch
/// `token` also catches `token_id`, and any pattern narrow enough to spare
/// `token_id` misses a future `access_token`. Allowlisting inverts the failure
/// mode: a parameter nobody has vetted is simply absent from the log.
///
/// Entries are identifiers and identifier collections only. Free-text and
/// structured payloads (`name`, `config`, `claims`, ...) are omitted on
/// purpose — a record states *what* was acted upon, not what it was changed to.
/// Each entry pairs the argument name with the log field it is emitted as.
/// The field name is namespaced under `params.` following the dotted
/// convention of OpenTelemetry semconv and Elastic Common Schema: it groups
/// parameters visually in text output, and guarantees a tool parameter can
/// never collide with a server-side field such as `tool` or `outcome`. The two
/// names live together so they cannot drift apart, and the field name is
/// `'static` because `Span::record` requires it.
const LOGGED_PARAMS: &[(&str, &str)] = &[
    ("user_id", "params.user_id"),
    ("client_id", "params.client_id"),
    ("group_id", "params.group_id"),
    ("image_type", "params.image_type"),
    ("api_id", "params.api_id"),
    ("provider_id", "params.provider_id"),
    ("token_id", "params.token_id"),
    ("key_id", "params.key_id"),
    ("credential_id", "params.credential_id"),
    ("user_ids", "params.user_ids"),
    ("user_group_ids", "params.user_group_ids"),
    ("oidc_client_ids", "params.oidc_client_ids"),
];

/// Longest collection rendered element-by-element; longer ones are summarized
/// so a bulk membership rewrite cannot emit a multi-kilobyte log line.
const MAX_LOGGED_COLLECTION: usize = 5;

/// Longest rendered value, in bytes. The element-count cap above bounds how
/// many items are rendered but not how long each is: these are identifiers by
/// contract, not by validation, and nothing stops a client sending a megabyte
/// string as `user_id` (or five of them in `user_ids`). Without this, one call
/// writes a megabyte to the audit log — the same denial-of-disk the collection
/// cap exists to prevent. Generous enough that a real identifier is never
/// touched.
const MAX_LOGGED_VALUE_LEN: usize = 256;

/// Truncate at a char boundary, marking that it happened so a truncated value
/// is never mistaken for the identifier actually sent.
fn truncate_value(mut s: String) -> String {
    if s.len() <= MAX_LOGGED_VALUE_LEN {
        return s;
    }
    let mut end = MAX_LOGGED_VALUE_LEN;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s.push_str("...(truncated)");
    s
}

/// The allowlisted parameters of a tool call, as `(log field name, value)`.
///
/// Each pair becomes its own log field, so `user_id` is queryable as
/// `params.user_id` rather than being encoded into a shared string. Returns an
/// empty vector when nothing is loggable, which the caller renders as no
/// parameter fields at all.
///
/// Only top-level keys are considered: every nested params struct uses
/// `#[serde(flatten)]`, so identifiers always arrive at the top level of the
/// arguments object, and anything genuinely nested (`config`, `claims`) is not
/// on the allowlist anyway.
pub fn loggable_params(
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Vec<(&'static str, String)> {
    let Some(arguments) = arguments else {
        return Vec::new();
    };
    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    // Iterate the allowlist, not the arguments, so output order is stable and
    // an unvetted key can never reach the log by any path.
    for (key, field) in LOGGED_PARAMS {
        let Some(value) = arguments.get(*key) else {
            continue;
        };
        match value {
            serde_json::Value::String(s) => pairs.push((field, truncate_value(s.clone()))),
            serde_json::Value::Array(items) => {
                let rendered = if items.len() > MAX_LOGGED_COLLECTION {
                    format!("[{} items]", items.len())
                } else {
                    let joined: Vec<String> = items
                        .iter()
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect();
                    // Truncate the joined rendering, so a handful of very long
                    // elements is bounded just as a long single value is.
                    truncate_value(format!("[{}]", joined.join(",")))
                };
                pairs.push((field, rendered));
            }
            // Numbers and booleans are harmless; anything structured is not
            // rendered, since an allowlisted identifier is never an object.
            serde_json::Value::Number(_) | serde_json::Value::Bool(_) => {
                pairs.push((field, value.to_string()))
            }
            _ => {}
        }
    }
    pairs
}

const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b'\\')
    .add(b'%');

/// Percent-encode a value used as a URL path segment.
pub fn seg(value: &str) -> String {
    utf8_percent_encode(value, PATH_SEGMENT).to_string()
}

/// Encode an OIDC client ID for use as a path segment.
///
/// CIMD client IDs are full https URLs containing slashes and colons, which
/// cannot travel in a single path segment; Pocket ID's API convention encodes
/// them as `~<base64url>`. Plain client IDs pass through unchanged.
pub fn client_seg(id: &str) -> String {
    use base64::Engine;
    if !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return id.to_string();
    }
    format!(
        "~{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(id.as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(json: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        match json {
            serde_json::Value::Object(map) => map,
            other => panic!("expected an object, got {other}"),
        }
    }

    #[test]
    fn every_catalog_tool_resolves_a_tier() {
        for tool in CATALOG {
            assert_eq!(
                tier_for(tool.name),
                Some(tool.tier),
                "no tier resolved for {}",
                tool.name
            );
        }
        assert_eq!(tier_for("no_such_tool"), None);
    }

    #[test]
    fn tier_renders_lowercase() {
        assert_eq!(Tier::Read.to_string(), "read");
        assert_eq!(Tier::Write.to_string(), "write");
        assert_eq!(Tier::Dangerous.to_string(), "dangerous");
    }

    /// All rendered values joined, for leak assertions.
    fn joined(pairs: &[(&'static str, String)]) -> String {
        pairs
            .iter()
            .map(|(f, v)| format!("{f}={v}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn loggable_params_keeps_identifiers() {
        let a = args(serde_json::json!({ "user_id": "abc123" }));
        // Each parameter is its own field, namespaced under `params.`.
        assert_eq!(
            loggable_params(Some(&a)),
            vec![("params.user_id", "abc123".to_string())]
        );
    }

    #[test]
    fn loggable_params_drops_secret_bearing_values() {
        // introspect_token's `token` is a live bearer token.
        let a = args(serde_json::json!({
            "token": "eyJhbGciOiJIUzI1NiJ9.secret-payload",
            "client_id": "my-client",
        }));
        let pairs = loggable_params(Some(&a));
        assert_eq!(pairs, vec![("params.client_id", "my-client".to_string())]);
        let rendered = joined(&pairs);
        assert!(!rendered.contains("secret-payload"));
        assert!(!rendered.contains("eyJ"));

        // A bare `key` is API key material; `key_id` merely identifies one.
        let a = args(serde_json::json!({ "key": "pid_live_abcdef", "key_id": "k-1" }));
        let pairs = loggable_params(Some(&a));
        assert_eq!(pairs, vec![("params.key_id", "k-1".to_string())]);
        assert!(!joined(&pairs).contains("pid_live_abcdef"));
    }

    #[test]
    fn loggable_params_keeps_id_suffixed_names_despite_prefixes() {
        // The denylist trap: these must survive even though `token` and `key`
        // must not.
        let a = args(serde_json::json!({ "token_id": "t-1" }));
        assert_eq!(
            loggable_params(Some(&a)),
            vec![("params.token_id", "t-1".to_string())]
        );
    }

    #[test]
    fn loggable_params_drops_non_allowlisted_and_nested() {
        let a = args(serde_json::json!({
            "name": "Production key",
            "ttl": 3600,
            "config": { "ldapBindPassword": "hunter2" },
            "claims": { "department": "eng" },
        }));
        assert!(loggable_params(Some(&a)).is_empty());
    }

    #[test]
    fn loggable_params_summarizes_large_collections() {
        let small = args(serde_json::json!({ "user_group_ids": ["g1", "g2"] }));
        assert_eq!(
            loggable_params(Some(&small)),
            vec![("params.user_group_ids", "[g1,g2]".to_string())]
        );

        let ids: Vec<String> = (0..500).map(|i| format!("g{i}")).collect();
        let large = args(serde_json::json!({ "user_group_ids": ids }));
        let pairs = loggable_params(Some(&large));
        assert_eq!(
            pairs,
            vec![("params.user_group_ids", "[500 items]".to_string())]
        );
        assert!(
            pairs[0].1.len() < 64,
            "collection summary must stay bounded"
        );
    }

    #[test]
    fn loggable_params_bounds_oversized_values() {
        // The element-count cap does not bound element *length*: an identifier
        // is one by contract, not by validation, so a client can send a huge
        // string under an allowlisted name.
        let huge = "x".repeat(100_000);
        let a = args(serde_json::json!({ "user_id": huge }));
        let pairs = loggable_params(Some(&a));
        assert!(
            pairs[0].1.len() < MAX_LOGGED_VALUE_LEN + 32,
            "oversized value not bounded: {} bytes",
            pairs[0].1.len()
        );
        assert!(pairs[0].1.ends_with("...(truncated)"));

        // A short collection of long elements passes the count cap, so the
        // joined rendering must be bounded too.
        let long_ids: Vec<String> = (0..3).map(|_| "y".repeat(10_000)).collect();
        let a = args(serde_json::json!({ "user_ids": long_ids }));
        let pairs = loggable_params(Some(&a));
        assert!(
            pairs[0].1.len() < MAX_LOGGED_VALUE_LEN + 32,
            "oversized collection not bounded: {} bytes",
            pairs[0].1.len()
        );
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        // Truncating mid-codepoint would panic; a multi-byte value must survive.
        let multibyte = "é".repeat(1_000);
        let a = args(serde_json::json!({ "user_id": multibyte }));
        let pairs = loggable_params(Some(&a));
        assert!(pairs[0].1.ends_with("...(truncated)"));

        // A value at exactly the limit is left untouched.
        let exact = "z".repeat(MAX_LOGGED_VALUE_LEN);
        let a = args(serde_json::json!({ "user_id": exact.clone() }));
        assert_eq!(loggable_params(Some(&a))[0].1, exact);
    }

    #[test]
    fn loggable_params_absent_when_nothing_allowlisted() {
        assert!(loggable_params(None).is_empty());
        let empty = args(serde_json::json!({}));
        assert!(loggable_params(Some(&empty)).is_empty());
    }

    #[test]
    fn loggable_params_order_is_stable() {
        // Driven by the allowlist's order, not the caller's key order.
        let a = args(serde_json::json!({ "group_id": "g", "user_id": "u" }));
        assert_eq!(
            loggable_params(Some(&a)),
            vec![
                ("params.user_id", "u".to_string()),
                ("params.group_id", "g".to_string()),
            ]
        );
    }

    #[test]
    fn every_allowlist_field_is_namespaced_and_matches_its_key() {
        // The two names live together precisely so they cannot drift.
        for (key, field) in LOGGED_PARAMS {
            assert_eq!(
                *field,
                format!("params.{key}"),
                "field name does not match its argument name"
            );
        }
    }

    #[test]
    fn catalog_names_are_unique() {
        let mut names: Vec<&str> = CATALOG.iter().map(|t| t.name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate tool names in catalog");
    }

    #[test]
    fn catalog_operations_are_unique() {
        let mut ops: Vec<(&str, &str)> = CATALOG
            .iter()
            .flat_map(|t| t.operations.iter().copied())
            .collect();
        let before = ops.len();
        ops.sort_unstable();
        ops.dedup();
        assert_eq!(before, ops.len(), "operation mapped by two tools");
    }

    #[test]
    fn seg_encodes_reserved() {
        assert_eq!(seg("plain-id_1.2"), "plain-id_1.2");
        assert_eq!(seg("a/b c%"), "a%2Fb%20c%25");
    }

    #[test]
    fn client_seg_encodes_cimd_urls() {
        // Plain client IDs pass through.
        assert_eq!(client_seg("my-client_1.2"), "my-client_1.2");
        // CIMD URL IDs become ~<base64url>, matching Pocket ID's convention.
        let encoded = client_seg("https://client.example.com/meta.json");
        assert!(encoded.starts_with('~'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
        use base64::Engine;
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&encoded[1..])
            .unwrap();
        assert_eq!(decoded, b"https://client.example.com/meta.json");
    }
}
