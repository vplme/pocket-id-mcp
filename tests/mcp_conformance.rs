//! MCP protocol conformance: the advertised tool definitions must validate
//! against the official MCP JSON Schema for every vendored protocol revision.
//!
//! Two revisions are vendored in `spec/` because clients negotiate the
//! revision downward, so definitions must satisfy the strictest negotiable
//! one — not just the newest:
//!
//! - `2025-06-18`: the strict floor. Requires `outputSchema` to be
//!   `{"type": "object"}` with object-valued `properties` — exactly what
//!   Claude Code's SDK-derived validation enforces.
//! - `2026-07-28`: the newest revision the pinned rmcp supports (it loosened
//!   `outputSchema` to any JSON Schema, but may add constraints elsewhere).
//!
//! Coupling rule: when bumping rmcp to a build supporting a newer protocol
//! revision, vendor that revision's schema.json here as well.

use std::collections::HashMap;
use std::sync::Arc;

use pocket_id_mcp::client::PocketIdClient;
use pocket_id_mcp::config::Config;
use pocket_id_mcp::server::PocketIdServer;
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

/// Validate `instance` against `definition` inside the vendored schema for
/// `revision`, returning one line per violation.
fn violations(revision: &str, definition: &str, instance: &serde_json::Value) -> Vec<String> {
    let path = format!(
        "{}/spec/mcp-schema-{revision}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("vendored schema {path} unreadable: {e}"));
    let mut schema: serde_json::Value = serde_json::from_str(&text).unwrap();
    // The schema documents define every type under definitions/$defs with no
    // root type; point the root at the definition under test.
    let defs_key = if schema.get("definitions").is_some() {
        "definitions"
    } else {
        "$defs"
    };
    schema["$ref"] = json!(format!("#/{defs_key}/{definition}"));
    let validator = jsonschema::validator_for(&schema)
        .unwrap_or_else(|e| panic!("schema {revision} failed to compile: {e}"));
    validator
        .iter_errors(instance)
        .map(|err| format!("{revision} {}: {err}", err.instance_path()))
        .collect()
}

#[test]
fn tool_definitions_conform_to_mcp_schema() {
    // Each tool is validated against the `Tool` definition rather than the
    // whole `ListToolsResult`: newer revisions add runtime envelope fields
    // (resultType, cacheScope, ttlMs) that the serving SDK stamps onto
    // responses — those are rmcp's responsibility, not the definitions'.
    let server = all_tiers_server();
    let tools = server.registered_tools();
    assert!(!tools.is_empty());
    let mut all = Vec::new();
    for revision in ["2025-06-18", "2026-07-28"] {
        for tool in &tools {
            let name = tool.name.clone();
            let instance = serde_json::to_value(tool).unwrap();
            all.extend(
                violations(revision, "Tool", &instance)
                    .into_iter()
                    .map(|v| format!("[{name}] {v}")),
            );
        }
    }
    assert!(
        all.is_empty(),
        "tool definitions violate the MCP schema:\n{}",
        all.join("\n")
    );
}

#[test]
fn validator_catches_known_bad_definitions() {
    // Negative control: both bug shapes that historically broke Claude Code
    // must be flagged by the strict revision, proving the net catches them.
    let array_rooted = json!({
        "name": "bad_array",
        "inputSchema": { "type": "object" },
        "outputSchema": { "type": "array", "items": { "type": "string" } },
    });
    let boolean_property = json!({
        "name": "bad_bool_prop",
        "inputSchema": { "type": "object" },
        "outputSchema": {
            "type": "object",
            "properties": { "result": true },
            "required": ["result"],
        },
    });
    for (label, tool) in [
        ("array-rooted outputSchema", &array_rooted),
        ("boolean property schema", &boolean_property),
    ] {
        assert!(
            !violations("2025-06-18", "Tool", tool).is_empty(),
            "{label} was not flagged by the 2025-06-18 schema"
        );
    }
}
