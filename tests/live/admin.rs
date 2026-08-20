//! Admin tools: API keys, branding images, application configuration, and
//! read-only status tools — verified against Pocket ID directly.

use base64::Engine;
use reqwest::Method;
use serde_json::{Value, json};

use crate::common::{LiveEnv, Mcp, Mode, fixture, has_id, str_of, structured, text_of, unique};

/// Pocket ID (2.13) refuses API-key authentication for key *creation* and
/// *renewal* (`403 api_key_auth_not_allowed`) — those need an admin session —
/// while listing and revocation are allowed. This pins that upstream contract
/// so the tool surface can be documented accurately; if creation ever starts
/// succeeding here, the tool descriptions and README should be updated.
#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn api_key_creation_is_refused_under_api_key_auth() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let name = unique("live-key");
    let text = mcp
        .call_err(
            "create_api_key",
            json!({"name": name, "expires_at": "2030-01-01T00:00:00Z"}),
        )
        .await;
    assert!(text.contains("403"), "error text: {text}");
    assert!(text.contains("not allowed"), "error text: {text}");
    let listed = env.get_ok("/api/api-keys").await;
    assert!(
        !listed["data"]
            .as_array()
            .is_some_and(|k| k.iter().any(|x| x["name"] == name)),
        "refused key must not exist: {listed}"
    );
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn revoked_api_key_stops_authenticating() {
    let env = LiveEnv::acquire().await;
    let Some(spare) = &env.spare_api_key else {
        eprintln!("skipping: no spare API key for a user-supplied instance");
        return;
    };
    let mcp = Mcp::spawn(env, Mode::DANGEROUS).await;

    // The spare key is on record and works as a credential...
    let listed = mcp.call_json("list_api_keys", Value::Null).await;
    assert!(has_id(&listed, &spare.id), "api keys via tool: {listed}");
    let (status, me) = env
        .send_with_key(Method::GET, "/api/users/me", None, &spare.token)
        .await;
    assert!(
        status.is_success(),
        "spare key rejected before revocation: {status} {me}"
    );

    // ...until revoked through the (dangerous-tier) tool.
    mcp.call("revoke_api_key", json!({"key_id": spare.id}))
        .await;
    let (status, _) = env
        .send_with_key(Method::GET, "/api/users/me", None, &spare.token)
        .await;
    assert_eq!(status, 401, "revoked key still accepted by Pocket ID");
    let listed = env.get_ok("/api/api-keys").await;
    assert!(
        !has_id(&listed, &spare.id),
        "revoked key still listed: {listed}"
    );
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn application_image_round_trips_byte_for_byte() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let path = fixture("logo.png");
    let expected = std::fs::read(&path).expect("fixture logo.png");

    // Dark-mode logo so the default (light) branding is left alone.
    mcp.call(
        "update_application_image",
        json!({"image_type": "logo", "light": false, "file_path": path.to_str().unwrap()}),
    )
    .await;

    let (status, content_type, bytes) = env
        .get_bytes("/api/application-images/logo?light=false")
        .await;
    assert!(status.is_success(), "GET logo -> {status}");
    assert!(
        content_type.starts_with("image/png"),
        "content type {content_type}"
    );
    assert_eq!(
        bytes, expected,
        "Pocket ID serves exactly the uploaded bytes"
    );

    // And the read tool returns the same image as an MCP image block.
    let result = mcp
        .call(
            "get_application_image",
            json!({"image_type": "logo", "light": false}),
        )
        .await;
    let image = result
        .content
        .iter()
        .find_map(|c| c.as_image())
        .unwrap_or_else(|| panic!("no image block in {}", text_of(&result)));
    assert_eq!(image.mime_type, "image/png");
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&image.data)
        .expect("base64 image data");
    assert_eq!(decoded, expected);
    mcp.shutdown().await;
}

/// `get_all_application_configuration` returns Pocket ID's array of
/// `{key, value}` entries; `update_application_configuration` wants the
/// complete flat object back.
#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn application_configuration_update_persists() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;

    let all = mcp
        .call_json("get_all_application_configuration", Value::Null)
        .await;
    let entries = all["result"].as_array().expect("config entries array");
    let mut flat: serde_json::Map<String, Value> = entries
        .iter()
        .map(|e| (str_of(e, "key").to_string(), e["value"].clone()))
        .collect();
    let original_name = flat["appName"].clone();
    let new_name = unique("Live Suite");
    flat.insert("appName".into(), Value::String(new_name.clone()));

    mcp.call(
        "update_application_configuration",
        json!({"config": Value::Object(flat.clone())}),
    )
    .await;
    let public = env.get_ok("/api/application-configuration").await;
    assert_eq!(config_value(&public, "appName"), Some(new_name.as_str()));

    // Restore so the instance is left as found.
    flat.insert("appName".into(), original_name.clone());
    mcp.call(
        "update_application_configuration",
        json!({"config": Value::Object(flat)}),
    )
    .await;
    let public = env.get_ok("/api/application-configuration").await;
    assert_eq!(
        config_value(&public, "appName"),
        original_name.as_str(),
        "appName restored"
    );
    mcp.shutdown().await;
}

fn config_value<'a>(entries: &'a Value, key: &str) -> Option<&'a str> {
    entries.as_array()?.iter().find(|e| e["key"] == key)?["value"].as_str()
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn status_tools_report_the_real_instance() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::READ_ONLY).await;

    let version = mcp.call_json("get_current_version", Value::Null).await;
    let current = str_of(&version["result"], "currentVersion");
    let upstream = env.get_ok("/api/version/current").await;
    assert_eq!(current, str_of(&upstream, "currentVersion"));
    if let Some(expected) = &env.expected_version {
        assert_eq!(current, expected, "container image version");
    }

    let health = mcp.call("health_check", Value::Null).await;
    assert!(!text_of(&health).is_empty());

    let logs = mcp
        .call_json("list_all_audit_logs", json!({"limit": 5}))
        .await;
    assert!(logs["data"].is_array(), "audit log page: {logs}");
    assert!(logs["pagination"].is_object(), "audit log page: {logs}");

    let users = mcp
        .call_json("list_users", json!({"search": "admin"}))
        .await;
    assert!(
        users["data"]
            .as_array()
            .is_some_and(|u| u.iter().any(|x| x["username"] == "admin")),
        "bootstrap admin visible through list_users: {users}"
    );
    // Sanity: structured() agrees with the wire-level structuredContent.
    let raw = mcp.call("list_users", json!({"search": "admin"})).await;
    assert_eq!(structured(&raw)["data"], users["data"]);
    mcp.shutdown().await;
}
