//! Strict tool input parameters: unknown top-level keys are rejected at
//! deserialization (`deny_unknown_fields`), the advertised schemas declare
//! `additionalProperties: false` to match, and closed value sets are plain
//! inline string enums.
//!
//! The flatten round-trips pin behavior serde documents as unsupported
//! (`deny_unknown_fields` + `flatten`) but that verifiably works today,
//! including on dual-role structs used both directly as `Parameters<T>` and
//! flattened into another deny struct. If a serde upgrade changes this, these
//! tests fail rather than silently reopening the silent-drop hole.

use std::collections::HashMap;
use std::sync::Arc;

use pocket_id_mcp::client::{FileSource, PocketIdClient};
use pocket_id_mcp::config::Config;
use pocket_id_mcp::dto::ListParams;
use pocket_id_mcp::server::PocketIdServer;
use pocket_id_mcp::tools::admin::{AuditLogFilterParams, UpdateImageParams};
use pocket_id_mcp::tools::identity::{SearchListParams, UserIdParam};
use serde_json::json;

fn all_tiers_server() -> PocketIdServer {
    let vars = HashMap::from([
        (
            "POCKET_ID_URL".to_string(),
            "https://id.example.com".to_string(),
        ),
        ("POCKET_ID_API_KEY".to_string(), "k".to_string()),
        (
            "POCKET_ID_MCP_ALLOW_DANGEROUS".to_string(),
            "true".to_string(),
        ),
    ]);
    let config = Arc::new(Config::from_vars(&vars).unwrap());
    let client = Arc::new(PocketIdClient::new("https://id.example.com", "k".into()));
    PocketIdServer::new(config, client)
}

/// Violations of the strict-input invariant for one tool's input schema:
/// top level must declare `additionalProperties: false`, and no subschema may
/// use `anyOf` (nullable wrappers around optional params must be collapsed so
/// form-rendering clients get a typed input). Nested object DTOs may still use
/// `$defs`/`$ref`; enum params are pinned as inline by the spot-check test.
fn strictness_violations(name: &str, schema: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if schema.get("additionalProperties") != Some(&json!(false)) {
        out.push(format!(
            "tool {name:?} does not advertise additionalProperties: false"
        ));
    }
    fn walk(name: &str, value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                if map.contains_key("anyOf") {
                    out.push(format!("tool {name:?} contains anyOf: {value}"));
                }
                map.values().for_each(|v| walk(name, v, out));
            }
            serde_json::Value::Array(items) => items.iter().for_each(|v| walk(name, v, out)),
            _ => {}
        }
    }
    walk(name, schema, &mut out);
    out
}

#[test]
fn input_schemas_advertise_strictness() {
    let server = all_tiers_server();
    let tools = server.registered_tools();
    assert!(!tools.is_empty());
    let violations: Vec<String> = tools
        .iter()
        .flat_map(|t| {
            let schema = serde_json::Value::Object((*t.input_schema).clone());
            strictness_violations(&t.name, &schema)
        })
        .collect();
    assert!(
        violations.is_empty(),
        "input schemas violate the strictness invariant:\n{}",
        violations.join("\n")
    );
}

#[test]
fn closed_value_sets_are_inline_enums() {
    let server = all_tiers_server();
    let tools = server.registered_tools();
    let prop = |tool: &str, prop: &str| -> serde_json::Value {
        tools
            .iter()
            .find(|t| t.name == tool)
            .unwrap_or_else(|| panic!("{tool} missing"))
            .input_schema
            .get("properties")
            .and_then(|p| p.get(prop))
            .unwrap_or_else(|| panic!("{tool}.{prop} missing"))
            .clone()
    };
    let sort = prop("list_users", "sort_direction");
    assert_eq!(sort.get("type"), Some(&json!("string")));
    assert_eq!(sort.get("enum"), Some(&json!(["asc", "desc"])));
    let location = prop("list_all_audit_logs", "location");
    assert_eq!(location.get("type"), Some(&json!("string")));
    assert_eq!(location.get("enum"), Some(&json!(["internal", "external"])));
}

#[test]
fn strictness_checker_catches_open_world_schema() {
    // Negative control: an open-world schema (no additionalProperties) and an
    // un-collapsed optional enum (anyOf) must both be flagged, proving the
    // invariant test above actually bites.
    let open_world = json!({ "type": "object", "properties": {} });
    let anyof = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": { "dir": { "anyOf": [
            { "type": "string", "enum": ["asc", "desc"] },
            { "type": "null" },
        ]}},
    });
    assert!(!strictness_violations("bad_open", &open_world).is_empty());
    assert!(!strictness_violations("bad_anyof", &anyof).is_empty());
}

#[test]
fn unknown_top_level_key_is_rejected() {
    // Plain struct.
    let err = serde_json::from_value::<UserIdParam>(json!({"user_id": "u1", "bogus": 1}))
        .expect_err("unknown key accepted on plain struct");
    assert!(err.to_string().contains("bogus"), "unhelpful error: {err}");
    // Flattened struct, with the exact wire-format mimicry observed live:
    // REST-style `filters`/`sort` objects instead of the flat params.
    serde_json::from_value::<AuditLogFilterParams>(
        json!({"filters": {"userId": "u1"}, "sort": {"column": "createdAt"}}),
    )
    .expect_err("REST-style filters/sort accepted on flattened struct");
}

#[test]
fn dual_role_structs_stay_strict_and_functional() {
    // ListParams: direct use.
    let ok = serde_json::from_value::<ListParams>(json!({"page": 1})).unwrap();
    assert_eq!(ok.page, Some(1));
    serde_json::from_value::<ListParams>(json!({"pgae": 1})).expect_err("typo accepted directly");
    // ListParams: flattened into SearchListParams (outer and inner both deny).
    let ok = serde_json::from_value::<SearchListParams>(json!({"search": "x", "page": 2})).unwrap();
    assert_eq!(ok.search.as_deref(), Some("x"));
    assert_eq!(ok.list.page, Some(2));
    serde_json::from_value::<SearchListParams>(json!({"search": "x", "pgae": 2}))
        .expect_err("typo accepted through flatten");
    // FileSource: direct use.
    serde_json::from_value::<FileSource>(json!({"file_path": "/tmp/a.png"})).unwrap();
    serde_json::from_value::<FileSource>(json!({"path": "/tmp/a.png"}))
        .expect_err("unknown key accepted on FileSource");
    // FileSource: flattened into UpdateImageParams.
    let ok = serde_json::from_value::<UpdateImageParams>(
        json!({"image_type": "logo", "file_path": "/tmp/a.png"}),
    )
    .unwrap();
    assert_eq!(ok.source.file_path.as_deref(), Some("/tmp/a.png"));
    serde_json::from_value::<UpdateImageParams>(
        json!({"image_type": "logo", "file_paht": "/tmp/a.png"}),
    )
    .expect_err("typo accepted through FileSource flatten");
}

#[test]
fn invalid_enum_value_error_names_the_valid_variants() {
    let err = serde_json::from_value::<ListParams>(json!({"sort_direction": "ascending"}))
        .expect_err("invalid sort direction accepted");
    let msg = err.to_string();
    assert!(
        msg.contains("asc") && msg.contains("desc"),
        "error does not name valid variants: {msg}"
    );
    let err = serde_json::from_value::<AuditLogFilterParams>(json!({"location": "outside"}))
        .expect_err("invalid location accepted");
    let msg = err.to_string();
    assert!(
        msg.contains("internal") && msg.contains("external"),
        "error does not name valid variants: {msg}"
    );
}
