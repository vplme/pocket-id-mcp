//! Spec coverage accounting: every operation in the vendored swagger spec must
//! be mapped to a tool in the catalog or excluded with a documented reason.

use std::collections::BTreeSet;

use pocket_id_mcp::tools::CATALOG;

fn swagger_operations() -> BTreeSet<(String, String)> {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/spec/swagger.yaml"))
        .expect("vendored spec readable");
    let spec: serde_yaml::Value = serde_yaml::from_str(&raw).expect("vendored spec parses");
    let paths = spec
        .get("paths")
        .and_then(|p| p.as_mapping())
        .expect("spec has paths");
    let mut ops = BTreeSet::new();
    for (path, item) in paths {
        let path = path.as_str().expect("path is a string");
        let item = item.as_mapping().expect("path item is a mapping");
        for (method, _) in item {
            let method = method.as_str().unwrap_or_default();
            if matches!(method, "get" | "post" | "put" | "delete" | "patch") {
                ops.insert((method.to_uppercase(), path.to_string()));
            }
        }
    }
    ops
}

#[derive(serde::Deserialize)]
struct Exclusions {
    exclusion: Vec<Exclusion>,
}

#[derive(serde::Deserialize)]
struct Exclusion {
    method: String,
    path: String,
    reason: String,
}

fn exclusions() -> Vec<Exclusion> {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/spec/exclusions.toml"))
        .expect("exclusions readable");
    toml::from_str::<Exclusions>(&raw)
        .expect("exclusions parse")
        .exclusion
}

#[test]
fn every_operation_is_mapped_or_excluded() {
    let spec_ops = swagger_operations();
    let mapped: BTreeSet<(String, String)> = CATALOG
        .iter()
        .flat_map(|t| t.operations.iter())
        .map(|(m, p)| (m.to_string(), p.to_string()))
        .collect();
    let excluded: BTreeSet<(String, String)> = exclusions()
        .iter()
        .map(|e| (e.method.clone(), e.path.clone()))
        .collect();

    let unmapped: Vec<_> = spec_ops
        .iter()
        .filter(|op| !mapped.contains(op) && !excluded.contains(op))
        .collect();
    assert!(
        unmapped.is_empty(),
        "operations in spec/swagger.yaml neither mapped to a tool nor excluded:\n{}",
        unmapped
            .iter()
            .map(|(m, p)| format!("  {m} {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn no_stale_mappings_or_exclusions() {
    let spec_ops = swagger_operations();
    let stale_mapped: Vec<String> = CATALOG
        .iter()
        .flat_map(|t| t.operations.iter().map(move |(m, p)| (t.name, m, p)))
        .filter(|(_, m, p)| !spec_ops.contains(&(m.to_string(), p.to_string())))
        .map(|(tool, m, p)| format!("  {tool}: {m} {p}"))
        .collect();
    assert!(
        stale_mapped.is_empty(),
        "catalog maps operations absent from the vendored spec:\n{}",
        stale_mapped.join("\n")
    );

    let stale_excluded: Vec<String> = exclusions()
        .iter()
        .filter(|e| !spec_ops.contains(&(e.method.clone(), e.path.clone())))
        .map(|e| format!("  {} {}", e.method, e.path))
        .collect();
    assert!(
        stale_excluded.is_empty(),
        "exclusion list contains operations absent from the vendored spec:\n{}",
        stale_excluded.join("\n")
    );
}

#[test]
fn no_operation_both_mapped_and_excluded() {
    let mapped: BTreeSet<(String, String)> = CATALOG
        .iter()
        .flat_map(|t| t.operations.iter())
        .map(|(m, p)| (m.to_string(), p.to_string()))
        .collect();
    let both: Vec<String> = exclusions()
        .iter()
        .filter(|e| mapped.contains(&(e.method.clone(), e.path.clone())))
        .map(|e| format!("  {} {}", e.method, e.path))
        .collect();
    assert!(both.is_empty(), "mapped AND excluded:\n{}", both.join("\n"));
}

#[test]
fn every_exclusion_has_a_reason() {
    for e in exclusions() {
        assert!(
            e.reason.trim().len() >= 10,
            "exclusion {} {} needs a real reason",
            e.method,
            e.path
        );
    }
}

/// The catalog and the actually-registered tool routers must agree: a catalog
/// entry without a corresponding `#[tool]` method (or vice versa) is a bug.
#[test]
fn catalog_matches_registered_tools() {
    use pocket_id_mcp::client::PocketIdClient;
    use pocket_id_mcp::config::Config;
    use pocket_id_mcp::server::PocketIdServer;
    use std::collections::HashMap;
    use std::sync::Arc;

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
    let client = Arc::new(PocketIdClient::new(
        &config.pocket_id_url,
        config.api_key.clone(),
    ));
    let server = PocketIdServer::new(config, client);

    let registered: BTreeSet<String> = server.registered_tool_names().into_iter().collect();
    let catalog: BTreeSet<String> = CATALOG.iter().map(|t| t.name.to_string()).collect();
    assert_eq!(
        registered, catalog,
        "registered tools and catalog disagree (left: registered, right: catalog)"
    );
}
