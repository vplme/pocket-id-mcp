//! Admin steps: API keys, branding images, application configuration, and
//! read-only status tools.

use std::str::FromStr;

use base64::Engine;
use cucumber::{Parameter, given, then, when};
use reqwest::Method;
use serde_json::{Value, json};

use crate::common::{fixture, has_id, str_of, text_of};
use crate::world::LiveWorld;

// --- API keys ----------------------------------------------------------------

#[when(expr = "I try to create an API key {string}")]
async fn try_create_api_key(w: &mut LiveWorld, name: String) {
    let name = w.expand(&name);
    w.last_error = Some(
        w.mcp()
            .call_err(
                "create_api_key",
                json!({"name": name, "expires_at": "2030-01-01T00:00:00Z"}),
            )
            .await,
    );
}

#[then(expr = "Pocket ID has no API key named {string}")]
async fn no_api_key_named(w: &mut LiveWorld, name: String) {
    let name = w.expand(&name);
    let listed = w.env.get_ok("/api/api-keys").await;
    assert!(
        !listed["data"]
            .as_array()
            .is_some_and(|k| k.iter().any(|x| x["name"] == name)),
        "key exists: {listed}"
    );
}

#[given("a spare API key minted at bootstrap")]
async fn spare_key(w: &mut LiveWorld) {
    assert!(
        w.env.spare_api_key.is_some(),
        "no spare key: this scenario needs the Docker bootstrap (tag it @needs-bootstrap)"
    );
}

#[then("the spare API key appears in the tool's API key list")]
async fn spare_listed_by_tool(w: &mut LiveWorld) {
    let spare = w.env.spare_api_key.as_ref().unwrap();
    let listed = w.mcp().call_json("list_api_keys", Value::Null).await;
    assert!(has_id(&listed, &spare.id), "api keys via tool: {listed}");
}

async fn spare_key_status(w: &LiveWorld) -> u16 {
    let spare = w.env.spare_api_key.as_ref().unwrap();
    w.env
        .send_with_key(Method::GET, "/api/users/me", None, &spare.token)
        .await
        .0
        .as_u16()
}

#[then("Pocket ID accepts the spare API key as a credential")]
async fn spare_accepted(w: &mut LiveWorld) {
    assert_eq!(spare_key_status(w).await, 200, "spare key rejected");
}

#[when("I try to renew the spare API key")]
async fn try_renew_spare(w: &mut LiveWorld) {
    let id = w.env.spare_api_key.as_ref().unwrap().id.clone();
    w.last_error = Some(
        w.mcp()
            .call_err("renew_api_key", json!({"key_id": id}))
            .await,
    );
}

#[when("I revoke the spare API key")]
async fn revoke_spare(w: &mut LiveWorld) {
    let id = w.env.spare_api_key.as_ref().unwrap().id.clone();
    w.mcp().call("revoke_api_key", json!({"key_id": id})).await;
}

#[then("Pocket ID rejects the spare API key as a credential")]
async fn spare_rejected(w: &mut LiveWorld) {
    assert_eq!(spare_key_status(w).await, 401, "revoked key still accepted");
}

#[then("Pocket ID no longer lists the spare API key")]
async fn spare_unlisted(w: &mut LiveWorld) {
    let spare = w.env.spare_api_key.as_ref().unwrap();
    let listed = w.env.get_ok("/api/api-keys").await;
    assert!(
        !has_id(&listed, &spare.id),
        "revoked key still listed: {listed}"
    );
}

// --- application images ------------------------------------------------------

/// Which branding image a step talks about.
#[derive(Debug, Clone, Copy, Parameter)]
#[param(
    name = "appimage",
    regex = "dark-mode logo|light-mode logo|background image"
)]
pub enum AppImage {
    DarkLogo,
    LightLogo,
    Background,
}

impl FromStr for AppImage {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "dark-mode logo" => AppImage::DarkLogo,
            "light-mode logo" => AppImage::LightLogo,
            "background image" => AppImage::Background,
            other => return Err(format!("unknown image {other}")),
        })
    }
}

impl AppImage {
    /// Tool arguments selecting this image (`image_type` + `light`).
    fn args(self) -> Value {
        match self {
            AppImage::DarkLogo => json!({"image_type": "logo", "light": false}),
            AppImage::LightLogo => json!({"image_type": "logo", "light": true}),
            AppImage::Background => json!({"image_type": "background"}),
        }
    }
    fn path(self) -> &'static str {
        match self {
            AppImage::DarkLogo => "/api/application-images/logo?light=false",
            AppImage::LightLogo => "/api/application-images/logo?light=true",
            AppImage::Background => "/api/application-images/background",
        }
    }
}

#[when(expr = "I upload {string} as the {appimage}")]
async fn upload_image(w: &mut LiveWorld, file: String, image: AppImage) {
    let path = fixture(&file);
    let mut args = image.args();
    args["file_path"] = json!(path.to_str().unwrap());
    w.mcp().call("update_application_image", args).await;
}

#[then(expr = "Pocket ID serves the {appimage} as image\\/png with exactly the bytes of {string}")]
async fn image_served(w: &mut LiveWorld, image: AppImage, file: String) {
    let expected = std::fs::read(fixture(&file)).expect("fixture");
    let (status, content_type, bytes) = w.env.get_bytes(image.path()).await;
    assert!(status.is_success(), "GET {image:?} -> {status}");
    assert!(
        content_type.starts_with("image/png"),
        "content type {content_type}"
    );
    assert_eq!(
        bytes, expected,
        "Pocket ID serves exactly the uploaded bytes"
    );
}

#[then(expr = "get_application_image returns the {appimage} with exactly the bytes of {string}")]
async fn image_via_tool(w: &mut LiveWorld, image: AppImage, file: String) {
    let expected = std::fs::read(fixture(&file)).expect("fixture");
    let result = w.mcp().call("get_application_image", image.args()).await;
    let block = result
        .content
        .iter()
        .find_map(|c| c.as_image())
        .unwrap_or_else(|| panic!("no image block in {}", text_of(&result)));
    assert_eq!(block.mime_type, "image/png");
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&block.data)
        .expect("base64 image data");
    assert_eq!(decoded, expected);
}

#[when(expr = "I delete the {appimage}")]
async fn delete_image(w: &mut LiveWorld, image: AppImage) {
    let mut args = image.args();
    args.as_object_mut().unwrap().remove("light");
    w.mcp().call("delete_application_image", args).await;
}

#[then(expr = "Pocket ID has no {appimage}")]
async fn no_image(w: &mut LiveWorld, image: AppImage) {
    let (status, _, _) = w.env.get_bytes(image.path()).await;
    assert_eq!(status, 404, "{image:?} still served");
}

// --- application configuration ----------------------------------------------

/// `get_all_application_configuration` returns Pocket ID's array of
/// `{key, value}` entries; `update_application_configuration` wants the
/// complete flat object back.
async fn put_app_name(w: &mut LiveWorld, name: Value) {
    if w.app_config.is_none() {
        let all = w
            .mcp()
            .call_json("get_all_application_configuration", Value::Null)
            .await;
        let flat: serde_json::Map<String, Value> = all["result"]
            .as_array()
            .expect("config entries array")
            .iter()
            .map(|e| (str_of(e, "key").to_string(), e["value"].clone()))
            .collect();
        w.original_app_name = Some(flat["appName"].clone());
        w.app_config = Some(flat);
    }
    let flat = w.app_config.as_mut().unwrap();
    flat.insert("appName".into(), name);
    let config = Value::Object(flat.clone());
    w.mcp()
        .call(
            "update_application_configuration",
            json!({"config": config}),
        )
        .await;
}

#[when(expr = "I change the application name to {string}")]
async fn change_app_name(w: &mut LiveWorld, name: String) {
    let name = w.expand(&name);
    put_app_name(w, Value::String(name)).await;
}

#[when("I restore the original application name")]
async fn restore_app_name(w: &mut LiveWorld) {
    let original = w
        .original_app_name
        .clone()
        .expect("a changed application name");
    put_app_name(w, original).await;
}

fn config_value<'a>(entries: &'a Value, key: &str) -> Option<&'a Value> {
    entries
        .as_array()?
        .iter()
        .find(|e| e["key"] == key)
        .map(|e| &e["value"])
}

#[then(expr = "Pocket ID's public configuration has appName {string}")]
async fn public_app_name(w: &mut LiveWorld, name: String) {
    let public = w.env.get_ok("/api/application-configuration").await;
    assert_eq!(
        config_value(&public, "appName"),
        Some(&json!(w.expand(&name)))
    );
}

#[then(expr = "get_public_application_configuration reports appName {string}")]
async fn tool_public_app_name(w: &mut LiveWorld, name: String) {
    let public = w
        .mcp()
        .call_json("get_public_application_configuration", Value::Null)
        .await;
    assert_eq!(
        config_value(&public["result"], "appName"),
        Some(&json!(w.expand(&name))),
        "public configuration via tool: {public}"
    );
}

#[then("Pocket ID's public configuration has the original appName")]
async fn public_app_name_original(w: &mut LiveWorld) {
    let public = w.env.get_ok("/api/application-configuration").await;
    assert_eq!(
        config_value(&public, "appName"),
        w.original_app_name.as_ref()
    );
}

// --- status tools ------------------------------------------------------------

#[then("get_current_version reports Pocket ID's own version")]
async fn version_matches(w: &mut LiveWorld) {
    let version = w.mcp().call_json("get_current_version", Value::Null).await;
    let current = str_of(&version["result"], "currentVersion");
    let upstream = w.env.get_ok("/api/version/current").await;
    assert_eq!(current, str_of(&upstream, "currentVersion"));
    if let Some(expected) = &w.env.expected_version {
        assert_eq!(current, expected, "container image version");
    }
}

#[then("get_latest_version reports a version")]
async fn latest_version(w: &mut LiveWorld) {
    let latest = w.mcp().call_json("get_latest_version", Value::Null).await;
    let v = str_of(&latest["result"], "latestVersion");
    assert!(
        v.starts_with(|c: char| c.is_ascii_digit()),
        "latest version: {v}"
    );
}

#[then("health_check succeeds")]
async fn health_ok(w: &mut LiveWorld) {
    let health = w.mcp().call("health_check", Value::Null).await;
    assert!(!text_of(&health).is_empty());
}

#[then(expr = "{word} returns an audit-log page")]
async fn audit_page(w: &mut LiveWorld, tool: String) {
    let logs = w.mcp().call_json(&tool, json!({"limit": 5})).await;
    assert!(logs["data"].is_array(), "{tool}: {logs}");
    assert!(logs["pagination"].is_object(), "{tool}: {logs}");
}

#[then("list_audit_log_users maps user ids to usernames")]
async fn audit_users(w: &mut LiveWorld) {
    let users = w.mcp().call_json("list_audit_log_users", Value::Null).await;
    let map = users["result"]
        .as_object()
        .unwrap_or_else(|| panic!("list_audit_log_users: {users}"));
    let upstream = w.env.get_ok("/api/audit-logs/filters/users").await;
    assert_eq!(Value::Object(map.clone()), upstream);
}

#[then("list_audit_log_client_names returns a list")]
async fn audit_client_names(w: &mut LiveWorld) {
    let names = w
        .mcp()
        .call_json("list_audit_log_client_names", Value::Null)
        .await;
    assert!(
        names["result"].is_array(),
        "list_audit_log_client_names: {names}"
    );
}

#[then("list_users finds the user that get_current_user reports")]
async fn list_finds_me(w: &mut LiveWorld) {
    let me = w.mcp().call_json("get_current_user", Value::Null).await;
    let username = str_of(&me, "username");
    let users = w
        .mcp()
        .call_json("list_users", json!({"search": username}))
        .await;
    assert!(has_id(&users, str_of(&me, "id")), "list_users: {users}");
}
