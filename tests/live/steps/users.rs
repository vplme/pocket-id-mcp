//! User steps.

use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use serde_json::{Value, json};

use crate::common::{has_id, str_of};
use crate::world::LiveWorld;

async fn create_user(w: &mut LiveWorld, args: Value) {
    let created = w.mcp().call_json("create_user", args).await;
    let id = str_of(&created, "id").to_string();
    w.cleanup.push(format!("/api/users/{id}"));
    w.user_id = Some(id);
}

#[when(expr = "I create a user {string} with:")]
async fn create_user_with_table(w: &mut LiveWorld, username: String, step: &Step) {
    let mut args = w.args_from_table("create_user", step);
    args.insert("username".into(), Value::String(w.expand(&username)));
    create_user(w, Value::Object(args)).await;
}

#[given(expr = "a user {string}")]
async fn given_user(w: &mut LiveWorld, username: String) {
    let username = w.expand(&username);
    create_user(
        w,
        json!({
            "username": username,
            "email": format!("{username}@example.com"),
            "firstName": "Live",
            "lastName": "Tester",
        }),
    )
    .await;
}

#[then("Pocket ID's record of that user has:")]
async fn user_record_has(w: &mut LiveWorld, step: &Step) {
    let stored = w.env.get_ok(&format!("/api/users/{}", w.user_id())).await;
    w.assert_table_matches(&stored, step);
}

#[then(expr = "that user appears when Pocket ID lists users matching {string}")]
async fn user_listed(w: &mut LiveWorld, search: String) {
    let listed = w
        .env
        .get_ok(&format!("/api/users?search={}", w.expand(&search)))
        .await;
    assert!(
        has_id(&listed, w.user_id()),
        "users listed by Pocket ID: {listed}"
    );
}

#[then(expr = "Pocket ID has no user named {string}")]
async fn no_user_named(w: &mut LiveWorld, username: String) {
    let listed = w
        .env
        .get_ok(&format!("/api/users?search={}", w.expand(&username)))
        .await;
    assert_eq!(listed["data"], json!([]), "unexpected users: {listed}");
}

#[when("I update that user with:")]
async fn update_user(w: &mut LiveWorld, step: &Step) {
    let mut args = w.args_from_table("update_user", step);
    args.insert("user_id".into(), Value::String(w.user_id().to_string()));
    w.mcp().call("update_user", Value::Object(args)).await;
}

// --- groups ------------------------------------------------------------------

#[when("I put that user in that group")]
async fn put_user_in_group(w: &mut LiveWorld) {
    w.mcp()
        .call(
            "set_user_groups",
            json!({"user_id": w.user_id(), "user_group_ids": [w.group_id()]}),
        )
        .await;
}

#[then("Pocket ID lists that group among that user's groups")]
async fn group_among_users_groups(w: &mut LiveWorld) {
    let groups = w
        .env
        .get_ok(&format!("/api/users/{}/groups", w.user_id()))
        .await;
    assert!(
        has_id(&groups, w.group_id()),
        "user's groups in Pocket ID: {groups}"
    );
}

#[then("Pocket ID lists that user among that group's members")]
async fn user_among_members(w: &mut LiveWorld) {
    let group = w
        .env
        .get_ok(&format!("/api/user-groups/{}", w.group_id()))
        .await;
    assert!(
        has_id(&group["users"], w.user_id()),
        "group members in Pocket ID: {}",
        group["users"]
    );
}

// --- custom claims -----------------------------------------------------------

#[when("I set that user's custom claims to:")]
async fn set_user_claims(w: &mut LiveWorld, step: &Step) {
    let claims = w.claims_from_table(step);
    w.mcp()
        .call(
            "update_user_custom_claims",
            json!({"user_id": w.user_id(), "claims": claims}),
        )
        .await;
}

#[then("Pocket ID's record of that user has custom claims:")]
async fn user_has_claims(w: &mut LiveWorld, step: &Step) {
    let stored = w.env.get_ok(&format!("/api/users/{}", w.user_id())).await;
    w.assert_claims(&stored, step);
}

// --- deletion ----------------------------------------------------------------

#[then(expr = "calling {string} on that user is refused by the protocol")]
async fn call_on_user_refused(w: &mut LiveWorld, tool: String) {
    let attempt = w
        .mcp()
        .try_call(&tool, json!({"user_id": w.user_id()}))
        .await;
    assert!(attempt.is_err(), "{tool} was callable: {attempt:?}");
}

#[when("I delete that user")]
async fn delete_user(w: &mut LiveWorld) {
    w.mcp()
        .call("delete_user", json!({"user_id": w.user_id()}))
        .await;
}

#[then("Pocket ID still has that user")]
async fn user_still_there(w: &mut LiveWorld) {
    let (status, body) = w.env.get(&format!("/api/users/{}", w.user_id())).await;
    assert!(status.is_success(), "user gone: {status} {body}");
}

#[then("Pocket ID no longer has that user")]
async fn user_gone(w: &mut LiveWorld) {
    let (status, body) = w.env.get(&format!("/api/users/{}", w.user_id())).await;
    assert_eq!(status, 404, "user still present after delete: {body}");
}
