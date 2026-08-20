//! User steps, including the current user ("me") tools, profile pictures,
//! passkeys, and the dangerous-tier signup / one-time-access tokens.

use base64::Engine;
use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use serde_json::{Value, json};

use crate::common::{fixture, has_id, str_of, text_of};
use crate::world::LiveWorld;

async fn create_user(w: &mut LiveWorld, args: Value) {
    let username = str_of(&args, "username").to_string();
    let created = w.mcp().call_json("create_user", args).await;
    let id = str_of(&created, "id").to_string();
    w.cleanup.push(format!("/api/users/{id}"));
    w.user_id = Some(id);
    w.user_name = Some(username);
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

/// Point "that user" at the API key's own user (no cleanup: it must stay).
#[given("that user is the current user")]
async fn that_user_is_me(w: &mut LiveWorld) {
    let me = w.mcp().call_json("get_current_user", Value::Null).await;
    w.user_id = Some(str_of(&me, "id").to_string());
    w.user_name = Some(str_of(&me, "username").to_string());
    w.me_before = Some(me);
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

// --- current user ("me") -----------------------------------------------------

/// `update_current_user` takes the full desired state, so username/email are
/// carried over from the record read at the start of the scenario.
async fn put_me(w: &mut LiveWorld, first_name: Value) {
    let before = w.me_before.clone().expect("that user is the current user");
    w.mcp()
        .call(
            "update_current_user",
            json!({
                "username": before["username"],
                "email": before["email"],
                "firstName": first_name,
                "lastName": before["lastName"],
            }),
        )
        .await;
}

#[when(expr = "I change the current user's first name to {string}")]
async fn change_my_first_name(w: &mut LiveWorld, name: String) {
    let name = w.expand(&name);
    put_me(w, Value::String(name)).await;
}

#[when("I restore the current user's first name")]
async fn restore_my_first_name(w: &mut LiveWorld) {
    let original =
        w.me_before.as_ref().expect("that user is the current user")["firstName"].clone();
    put_me(w, original).await;
}

#[then("Pocket ID's record of that user has its original first name")]
async fn my_first_name_restored(w: &mut LiveWorld) {
    let record = w.env.get_ok(&format!("/api/users/{}", w.user_id())).await;
    let original = &w.me_before.as_ref().unwrap()["firstName"];
    assert_eq!(&record["firstName"], original);
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

#[then("list_user_groups_of_user lists that group for that user")]
async fn tool_lists_users_groups(w: &mut LiveWorld) {
    let groups = w
        .mcp()
        .call_json("list_user_groups_of_user", json!({"user_id": w.user_id()}))
        .await;
    let groups = groups.get("result").cloned().unwrap_or(groups);
    assert!(
        has_id(&groups, w.group_id()),
        "list_user_groups_of_user: {groups}"
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

#[then(expr = "get_custom_claim_suggestions includes {string}")]
async fn suggestions_include(w: &mut LiveWorld, key: String) {
    let suggestions = w
        .mcp()
        .call_json("get_custom_claim_suggestions", Value::Null)
        .await;
    let list = suggestions["result"]
        .as_array()
        .unwrap_or_else(|| panic!("suggestions: {suggestions}"));
    assert!(
        list.iter().any(|k| k == &json!(key)),
        "suggestions: {list:?}"
    );
}

// --- passkeys ----------------------------------------------------------------

#[then("list_user_passkeys reports no passkeys for that user")]
async fn no_passkeys(w: &mut LiveWorld) {
    let passkeys = w
        .mcp()
        .call_json("list_user_passkeys", json!({"user_id": w.user_id()}))
        .await;
    assert_eq!(passkeys["result"], json!([]), "passkeys: {passkeys}");
    let record = w
        .env
        .get_ok(&format!("/api/users/{}/webauthn-credentials", w.user_id()))
        .await;
    assert_eq!(record, json!([]));
}

// --- profile pictures --------------------------------------------------------

async fn picture_bytes(w: &LiveWorld) -> Vec<u8> {
    let (status, content_type, bytes) = w
        .env
        .get_bytes(&format!("/api/users/{}/profile-picture.png", w.user_id()))
        .await;
    assert!(status.is_success(), "GET profile picture -> {status}");
    assert!(
        content_type.starts_with("image/png"),
        "content type {content_type}"
    );
    bytes
}

async fn remember_picture(w: &mut LiveWorld) {
    let before = picture_bytes(w).await;
    w.picture_before = Some(before);
}

#[when(expr = "I upload {string} as that user's profile picture")]
async fn upload_user_picture(w: &mut LiveWorld, file: String) {
    remember_picture(w).await;
    let path = fixture(&file);
    w.mcp()
        .call(
            "update_user_profile_picture",
            json!({"user_id": w.user_id(), "file_path": path.to_str().unwrap()}),
        )
        .await;
}

#[when(expr = "I upload {string} as the current user's profile picture")]
async fn upload_my_picture(w: &mut LiveWorld, file: String) {
    remember_picture(w).await;
    let path = fixture(&file);
    w.mcp()
        .call(
            "update_current_user_profile_picture",
            json!({"file_path": path.to_str().unwrap()}),
        )
        .await;
}

/// Pocket ID re-encodes uploaded profile pictures, so "stored" is proven by
/// the served picture changing from (and later back to) the default.
#[then("Pocket ID serves a different profile picture for that user than before")]
async fn picture_changed(w: &mut LiveWorld) {
    let now = picture_bytes(w).await;
    assert_ne!(
        Some(&now),
        w.picture_before.as_ref(),
        "profile picture unchanged"
    );
}

#[then("get_user_profile_picture returns the picture Pocket ID serves for that user")]
async fn picture_via_tool(w: &mut LiveWorld) {
    let served = picture_bytes(w).await;
    let result = w
        .mcp()
        .call("get_user_profile_picture", json!({"user_id": w.user_id()}))
        .await;
    let image = result
        .content
        .iter()
        .find_map(|c| c.as_image())
        .unwrap_or_else(|| panic!("no image block in {}", text_of(&result)));
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&image.data)
        .expect("base64 image data");
    assert_eq!(decoded, served);
}

#[when("I reset that user's profile picture")]
async fn reset_user_picture(w: &mut LiveWorld) {
    w.mcp()
        .call(
            "reset_user_profile_picture",
            json!({"user_id": w.user_id()}),
        )
        .await;
}

#[when("I reset the current user's profile picture")]
async fn reset_my_picture(w: &mut LiveWorld) {
    w.mcp()
        .call("reset_current_user_profile_picture", Value::Null)
        .await;
}

#[then("Pocket ID serves that user's default profile picture again")]
async fn picture_restored(w: &mut LiveWorld) {
    let now = picture_bytes(w).await;
    assert_eq!(
        Some(&now),
        w.picture_before.as_ref(),
        "default picture not restored"
    );
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

// --- signup tokens (dangerous tier) ------------------------------------------

#[when(expr = "I create a signup token valid for {string} with usage limit {int}")]
async fn create_signup_token(w: &mut LiveWorld, ttl: String, usage_limit: i64) {
    let token = w
        .mcp()
        .call_json(
            "create_signup_token",
            json!({"ttl": ttl, "usage_limit": usage_limit}),
        )
        .await;
    let id = str_of(&token, "id").to_string();
    assert_eq!(token["usageLimit"], usage_limit);
    w.cleanup.push(format!("/api/signup-tokens/{id}"));
    w.signup_token_id = Some(id);
}

fn signup_token_id(w: &LiveWorld) -> &str {
    w.signup_token_id
        .as_deref()
        .expect("a signup token created earlier")
}

#[then("list_signup_tokens lists that signup token")]
async fn tool_lists_signup_token(w: &mut LiveWorld) {
    let listed = w
        .mcp()
        .call_json("list_signup_tokens", json!({"limit": 100}))
        .await;
    assert!(
        has_id(&listed, signup_token_id(w)),
        "list_signup_tokens: {listed}"
    );
}

#[then("Pocket ID lists that signup token")]
async fn pocket_id_lists_signup_token(w: &mut LiveWorld) {
    let listed = w.env.get_ok("/api/signup-tokens?limit=100").await;
    assert!(
        has_id(&listed, signup_token_id(w)),
        "signup tokens: {listed}"
    );
}

#[when("I delete that signup token")]
async fn delete_signup_token(w: &mut LiveWorld) {
    let id = signup_token_id(w).to_string();
    w.mcp()
        .call("delete_signup_token", json!({"token_id": id}))
        .await;
}

#[then("Pocket ID no longer lists that signup token")]
async fn signup_token_gone(w: &mut LiveWorld) {
    let listed = w.env.get_ok("/api/signup-tokens?limit=100").await;
    assert!(
        !has_id(&listed, signup_token_id(w)),
        "signup tokens: {listed}"
    );
}

// --- one-time access token (dangerous tier) ----------------------------------

#[when("I mint a one-time access token for that user")]
async fn mint_one_time_token(w: &mut LiveWorld) {
    let minted = w
        .mcp()
        .call_json(
            "create_one_time_access_token",
            json!({"user_id": w.user_id()}),
        )
        .await;
    // Freeform upstream payload, enveloped as {"result": {...}}.
    w.one_time_token = Some(str_of(&minted["result"], "token").to_string());
}

/// Redeeming the token is the browser-side login step (excluded from the
/// tool surface); Pocket ID accepts it exactly once.
#[then("Pocket ID redeems that token exactly once")]
async fn redeem_once(w: &mut LiveWorld) {
    let token = w.one_time_token.clone().expect("a minted one-time token");
    let redeem = || {
        reqwest::Client::new()
            .post(w.env.url(&format!("/api/one-time-access-token/{token}")))
            .send()
    };
    let first = redeem().await.expect("redeem request").status().as_u16();
    assert_eq!(first, 200, "first redemption refused");
    let second = redeem().await.expect("redeem request").status().as_u16();
    assert_ne!(second, 200, "token redeemable twice");
}
