//! Safety-tier registration tests: the registered tool set must match the
//! catalog exactly for every configuration.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use pocket_id_mcp::client::PocketIdClient;
use pocket_id_mcp::config::Config;
use pocket_id_mcp::server::PocketIdServer;
use pocket_id_mcp::tools::{CATALOG, Tier};

fn server_with(read_only: bool, allow_dangerous: bool) -> PocketIdServer {
    let mut vars = HashMap::from([
        (
            "POCKET_ID_URL".to_string(),
            "https://id.example.com".to_string(),
        ),
        ("POCKET_ID_API_KEY".to_string(), "k".to_string()),
    ]);
    if read_only {
        vars.insert("POCKET_ID_MCP_READ_ONLY".to_string(), "true".to_string());
    }
    if allow_dangerous {
        vars.insert(
            "POCKET_ID_MCP_ALLOW_DANGEROUS".to_string(),
            "true".to_string(),
        );
    }
    let config = Arc::new(Config::from_vars(&vars).unwrap());
    let client = Arc::new(PocketIdClient::new(
        &config.pocket_id_url,
        config.api_key.clone(),
    ));
    PocketIdServer::new(config, client)
}

fn catalog_names(tiers: &[Tier]) -> BTreeSet<String> {
    CATALOG
        .iter()
        .filter(|t| tiers.contains(&t.tier))
        .map(|t| t.name.to_string())
        .collect()
}

fn registered(server: &PocketIdServer) -> BTreeSet<String> {
    server.registered_tool_names().into_iter().collect()
}

#[test]
fn default_config_registers_read_and_write_only() {
    let server = server_with(false, false);
    assert_eq!(
        registered(&server),
        catalog_names(&[Tier::Read, Tier::Write])
    );
}

#[test]
fn dangerous_tools_hidden_by_default() {
    let server = server_with(false, false);
    let names = registered(&server);
    for tool in [
        "delete_user",
        "delete_user_passkey",
        "create_one_time_access_token",
        "send_one_time_access_email",
        "request_one_time_access_email",
        "create_signup_token",
        "delete_signup_token",
        "revoke_api_key",
    ] {
        assert!(!names.contains(tool), "{tool} should be hidden by default");
    }
}

#[test]
fn read_only_registers_only_read_tier() {
    let server = server_with(true, false);
    assert_eq!(registered(&server), catalog_names(&[Tier::Read]));
}

#[test]
fn read_only_wins_over_allow_dangerous() {
    let server = server_with(true, true);
    assert_eq!(registered(&server), catalog_names(&[Tier::Read]));
}

#[test]
fn all_tiers_when_dangerous_enabled() {
    let server = server_with(false, true);
    assert_eq!(
        registered(&server),
        catalog_names(&[Tier::Read, Tier::Write, Tier::Dangerous])
    );
}

#[test]
fn write_prompts_hidden_in_read_only_mode() {
    let full = server_with(false, false);
    let read_only = server_with(true, false);
    let full_prompts: BTreeSet<_> = full.registered_prompt_names().into_iter().collect();
    let ro_prompts: BTreeSet<_> = read_only.registered_prompt_names().into_iter().collect();
    assert!(full_prompts.contains("onboard-oidc-client"));
    assert!(full_prompts.contains("audit-user-access"));
    assert!(full_prompts.contains("instance-health-check"));
    assert!(!ro_prompts.contains("onboard-oidc-client"));
    assert!(ro_prompts.contains("audit-user-access"));
}

#[test]
fn tools_with_structured_responses_declare_output_schema() {
    // Spot-check: JSON-returning tools advertise an output schema; image and
    // plain-text tools do not need one.
    let server = server_with(false, true);
    let tools = server.registered_tools();
    let by_name: HashMap<_, _> = tools.iter().map(|t| (t.name.as_ref(), t)).collect();
    for name in [
        "list_users",
        "get_oidc_client",
        "list_api_keys",
        "list_all_audit_logs",
    ] {
        let tool = by_name
            .get(name)
            .unwrap_or_else(|| panic!("{name} missing"));
        assert!(tool.output_schema.is_some(), "{name} lacks output schema");
    }
}

#[test]
fn input_schemas_use_plain_types_for_optional_params() {
    // schemars maps `Option<T>` to `"type": ["T", "null"]`; MCP Inspector and
    // other form-rendering clients only produce typed inputs (boolean toggle,
    // number field) for a plain `"type"` string. The server collapses the
    // arrays at registration, so no advertised input schema may contain one.
    fn assert_no_type_arrays(name: &str, value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(t) = map.get("type") {
                    assert!(!t.is_array(), "tool {name:?} advertises a type array: {t}");
                }
                map.values().for_each(|v| assert_no_type_arrays(name, v));
            }
            serde_json::Value::Array(items) => {
                items.iter().for_each(|v| assert_no_type_arrays(name, v));
            }
            _ => {}
        }
    }
    let server = server_with(false, true);
    for tool in server.registered_tools() {
        let schema = serde_json::Value::Object((*tool.input_schema).clone());
        assert_no_type_arrays(&tool.name, &schema);
    }
    // Spot-check the shape Inspector needs: an optional bool is a plain boolean.
    let tools = server.registered_tools();
    let logo = tools
        .iter()
        .find(|t| t.name == "get_oidc_client_logo")
        .expect("get_oidc_client_logo missing");
    let light_type = logo
        .input_schema
        .get("properties")
        .and_then(|p| p.get("light"))
        .and_then(|l| l.get("type"))
        .and_then(|t| t.as_str());
    assert_eq!(light_type, Some("boolean"));
}

#[test]
fn all_output_schemas_are_object_rooted() {
    // The MCP Tool type requires outputSchema to be `{"type": "object",
    // properties?: {[key]: object}}`; strict clients (Claude Code) reject the
    // entire tool list otherwise. Array/freeform results must use the
    // {"result": ...} envelope, and freeform values must use AnyJson so their
    // property schema is `{}` rather than schemars' boolean `true`.
    let server = server_with(false, true);
    for tool in server.registered_tools() {
        if let Some(schema) = &tool.output_schema {
            assert_eq!(
                schema.get("type").and_then(|t| t.as_str()),
                Some("object"),
                "tool {:?} declares a non-object outputSchema root: {}",
                tool.name,
                serde_json::to_string(schema).unwrap_or_default(),
            );
            if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                for (key, value) in props {
                    assert!(
                        value.is_object(),
                        "tool {:?} property {key:?} has a non-object schema: {value}",
                        tool.name,
                    );
                }
            }
        }
    }
}

/// "Current user" upstream is always the API key's service account, so the
/// self-service tools are pulled from HTTP OAuth mode, where callers are
/// distinct admitted users — and kept everywhere the operator plausibly IS
/// the service account.
#[test]
fn self_service_tools_not_registered_in_oauth_mode() {
    let self_service = [
        "get_current_user",
        "update_current_user",
        "update_current_user_profile_picture",
        "reset_current_user_profile_picture",
        "send_current_user_email_verification",
        "verify_current_user_email",
        "list_my_authorized_clients",
        "revoke_my_authorized_client",
        "list_my_accessible_clients",
        "list_my_audit_logs",
    ];

    let base = || {
        HashMap::from([
            (
                "POCKET_ID_URL".to_string(),
                "https://id.example.com".to_string(),
            ),
            ("POCKET_ID_API_KEY".to_string(), "k".to_string()),
            ("POCKET_ID_MCP_TRANSPORT".to_string(), "http".to_string()),
        ])
    };
    let build = |vars: HashMap<String, String>| {
        let config = Arc::new(Config::from_vars(&vars).unwrap());
        let client = Arc::new(PocketIdClient::new(
            &config.pocket_id_url,
            config.api_key.clone(),
        ));
        PocketIdServer::new(config, client)
    };

    let mut oauth_vars = base();
    oauth_vars.insert(
        "POCKET_ID_MCP_PUBLIC_URL".to_string(),
        "https://mcp.example.com".to_string(),
    );
    let oauth_names: BTreeSet<_> = build(oauth_vars)
        .registered_tool_names()
        .into_iter()
        .collect();

    let mut token_vars = base();
    token_vars.insert("POCKET_ID_MCP_HTTP_AUTH".to_string(), "token".to_string());
    token_vars.insert("POCKET_ID_MCP_HTTP_TOKEN".to_string(), "s3cret".to_string());
    let token_names: BTreeSet<_> = build(token_vars)
        .registered_tool_names()
        .into_iter()
        .collect();

    let stdio_names: BTreeSet<_> = server_with(false, false)
        .registered_tool_names()
        .into_iter()
        .collect();

    for name in self_service {
        assert!(
            !oauth_names.contains(name),
            "{name} registered in oauth mode"
        );
        assert!(token_names.contains(name), "{name} missing in token mode");
        assert!(stdio_names.contains(name), "{name} missing on stdio");
    }
    // Nothing else is affected by the OAuth-mode filtering.
    let removed: BTreeSet<_> = stdio_names.difference(&oauth_names).cloned().collect();
    let expected: BTreeSet<_> = self_service.iter().map(|s| s.to_string()).collect();
    assert_eq!(removed, expected);
}
