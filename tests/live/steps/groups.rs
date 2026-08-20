//! User-group steps.

use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use serde_json::{Value, json};

use crate::common::{has_id, str_of};
use crate::world::LiveWorld;

async fn create_group(w: &mut LiveWorld, name: &str, friendly_name: &str) {
    let group = w
        .mcp()
        .call_json(
            "create_group",
            json!({"name": name, "friendlyName": friendly_name}),
        )
        .await;
    let id = str_of(&group, "id").to_string();
    w.cleanup.push(format!("/api/user-groups/{id}"));
    w.group_id = Some(id);
    w.group_name = Some(name.to_string());
}

#[when(expr = "I create a user group {string} with friendly name {string}")]
async fn create_group_with_friendly(w: &mut LiveWorld, name: String, friendly: String) {
    let name = w.expand(&name);
    create_group(w, &name, &friendly).await;
}

#[given(expr = "a user group {string}")]
async fn given_group(w: &mut LiveWorld, name: String) {
    let name = w.expand(&name);
    create_group(w, &name, "Live Group").await;
}

#[when("I update that group with:")]
async fn update_group(w: &mut LiveWorld, step: &Step) {
    let mut args = w.args_from_table("update_group", step);
    args.insert("group_id".into(), Value::String(w.group_id().to_string()));
    if let Some(Value::String(name)) = args.get("name") {
        w.group_name = Some(name.clone());
    }
    w.mcp().call("update_group", Value::Object(args)).await;
}

// --- members -----------------------------------------------------------------

#[when("I set that group's members to that user")]
async fn set_members_to_user(w: &mut LiveWorld) {
    w.mcp()
        .call(
            "set_group_users",
            json!({"group_id": w.group_id(), "user_ids": [w.user_id()]}),
        )
        .await;
}

#[when("I clear that group's members")]
async fn clear_members(w: &mut LiveWorld) {
    w.mcp()
        .call(
            "set_group_users",
            json!({"group_id": w.group_id(), "user_ids": []}),
        )
        .await;
}

#[then("Pocket ID lists no members for that group")]
async fn no_members(w: &mut LiveWorld) {
    let stored = w
        .env
        .get_ok(&format!("/api/user-groups/{}", w.group_id()))
        .await;
    assert_eq!(stored["users"], json!([]), "members: {}", stored["users"]);
    assert!(!has_id(&stored["users"], w.user_id()));
}

// --- custom claims -----------------------------------------------------------

#[when("I set that group's custom claims to:")]
async fn set_group_claims(w: &mut LiveWorld, step: &Step) {
    let claims = w.claims_from_table(step);
    w.mcp()
        .call(
            "update_group_custom_claims",
            json!({"group_id": w.group_id(), "claims": claims}),
        )
        .await;
}

#[then("Pocket ID's record of that group has custom claims:")]
async fn group_has_claims(w: &mut LiveWorld, step: &Step) {
    let stored = w
        .env
        .get_ok(&format!("/api/user-groups/{}", w.group_id()))
        .await;
    w.assert_claims(&stored, step);
}

// --- deletion ----------------------------------------------------------------

#[when("I delete that group")]
async fn delete_group(w: &mut LiveWorld) {
    w.mcp()
        .call("delete_group", json!({"group_id": w.group_id()}))
        .await;
}
