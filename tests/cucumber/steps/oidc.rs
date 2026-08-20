//! OIDC client steps. `When`/`Given` act through tools; `Then` reads Pocket
//! ID back over REST (or proves a write-only value by using it).

use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use serde_json::{Value, json};

use crate::common::{has_id, str_of};
use crate::world::LiveWorld;

// --- creating clients ------------------------------------------------------

async fn create_client(w: &mut LiveWorld, name: &str, public: bool, pkce: bool, callback: &str) {
    let created = w
        .mcp()
        .call_json(
            "create_oidc_client",
            json!({
                "name": name,
                "description": "created by the live suite",
                "callbackURLs": [callback],
                "pkceEnabled": pkce,
                "isPublic": public,
            }),
        )
        .await;
    let id = str_of(&created, "id").to_string();
    w.cleanup.push(format!("/api/oidc/clients/{id}"));
    w.client_id = Some(id);
}

#[when(expr = "I create a confidential OIDC client {string} with PKCE and callback {string}")]
async fn create_confidential_with_callback(w: &mut LiveWorld, name: String, callback: String) {
    let name = w.expand(&name);
    create_client(w, &name, false, true, &callback).await;
}

#[given(expr = "a confidential OIDC client {string}")]
async fn given_confidential(w: &mut LiveWorld, name: String) {
    let name = w.expand(&name);
    create_client(w, &name, false, true, "https://app.example.com/callback").await;
}

#[given(expr = "a public OIDC client {string}")]
async fn given_public(w: &mut LiveWorld, name: String) {
    let name = w.expand(&name);
    create_client(w, &name, true, true, "https://app.example.com/callback").await;
}

#[then(expr = "Pocket ID has an OIDC client {string} with PKCE enabled and callback {string}")]
async fn pocket_id_has_client(w: &mut LiveWorld, name: String, callback: String) {
    let stored = w
        .env
        .get_ok(&format!("/api/oidc/clients/{}", w.client_id()))
        .await;
    assert_eq!(stored["name"], w.expand(&name));
    assert_eq!(stored["pkceEnabled"], true);
    assert_eq!(stored["isPublic"], false);
    assert_eq!(stored["callbackURLs"], json!([callback]));
}

#[then(expr = "that client appears when Pocket ID lists clients matching {string}")]
async fn client_listed(w: &mut LiveWorld, search: String) {
    let listed = w
        .env
        .get_ok(&format!("/api/oidc/clients?search={}", w.expand(&search)))
        .await;
    assert!(
        has_id(&listed, w.client_id()),
        "clients listed by Pocket ID: {listed}"
    );
}

// --- updating ----------------------------------------------------------------

#[when("I update that client with:")]
async fn update_client(w: &mut LiveWorld, step: &Step) {
    let mut args = w.args_from_table("update_oidc_client", step);
    args.insert("client_id".into(), Value::String(w.client_id().to_string()));
    w.mcp()
        .call("update_oidc_client", Value::Object(args))
        .await;
}

#[then("Pocket ID's record of that client has:")]
async fn client_record_has(w: &mut LiveWorld, step: &Step) {
    let stored = w
        .env
        .get_ok(&format!("/api/oidc/clients/{}", w.client_id()))
        .await;
    w.assert_table_matches(&stored, step);
}

// --- secrets -----------------------------------------------------------------

#[when(expr = "I set its secret to {string}")]
async fn set_secret(w: &mut LiveWorld, secret: String) {
    let secret = w.expand(&secret);
    let set = w
        .mcp()
        .call_json(
            "create_oidc_client_secret",
            json!({"client_id": w.client_id(), "secret": secret}),
        )
        .await;
    assert_eq!(set["secret"], secret, "chosen secret echoed back once");
    w.secret = Some(secret);
}

#[when("I rotate its secret")]
async fn rotate_secret(w: &mut LiveWorld) {
    let rotated = w
        .mcp()
        .call_json(
            "create_oidc_client_secret",
            json!({"client_id": w.client_id()}),
        )
        .await;
    let generated = str_of(&rotated, "secret").to_string();
    assert!(
        generated.len() >= 16,
        "generated secret too short: {generated:?}"
    );
    assert_ne!(
        Some(&generated),
        w.secret.as_ref(),
        "rotation produced the same secret"
    );
    w.secret = Some(generated);
}

/// Pocket ID's introspection endpoint authenticates the client with HTTP
/// Basic `client_id:secret`: 200 for valid credentials, 401 otherwise —
/// regardless of the token being introspected.
async fn introspect_status(w: &LiveWorld, secret: &str) -> u16 {
    reqwest::Client::new()
        .post(w.env.url("/api/oidc/introspect"))
        .basic_auth(w.client_id(), Some(secret))
        .form(&[("token", "not-a-real-token")])
        .send()
        .await
        .expect("introspect request")
        .status()
        .as_u16()
}

#[then(expr = "Pocket ID accepts {string} as that client's credential")]
async fn accepts_secret(w: &mut LiveWorld, secret: String) {
    let secret = w.expand(&secret);
    assert_eq!(
        introspect_status(w, &secret).await,
        200,
        "secret {secret:?} rejected"
    );
}

#[then("Pocket ID accepts the new secret as that client's credential")]
async fn accepts_new_secret(w: &mut LiveWorld) {
    let secret = w
        .secret
        .clone()
        .expect("a secret set earlier in the scenario");
    assert_eq!(
        introspect_status(w, &secret).await,
        200,
        "new secret rejected"
    );
}

#[then(expr = "Pocket ID rejects {string} as that client's credential")]
async fn rejects_secret(w: &mut LiveWorld, secret: String) {
    let secret = w.expand(&secret);
    assert_eq!(
        introspect_status(w, &secret).await,
        401,
        "secret {secret:?} accepted"
    );
}

// --- group restriction -------------------------------------------------------

#[given(expr = "a user group {string}")]
async fn given_group(w: &mut LiveWorld, name: String) {
    let group = w
        .mcp()
        .call_json(
            "create_group",
            json!({"name": w.expand(&name), "friendlyName": "Live Group"}),
        )
        .await;
    let id = str_of(&group, "id").to_string();
    w.cleanup.push(format!("/api/user-groups/{id}"));
    w.group_id = Some(id);
}

#[when("I restrict that client to that group")]
async fn restrict_to_group(w: &mut LiveWorld) {
    w.mcp()
        .call(
            "update_oidc_client_allowed_groups",
            json!({"client_id": w.client_id(), "user_group_ids": [w.group_id()]}),
        )
        .await;
}

#[when("I lift the group restriction on that client")]
async fn lift_restriction(w: &mut LiveWorld) {
    w.mcp()
        .call(
            "update_oidc_client_allowed_groups",
            json!({"client_id": w.client_id(), "user_group_ids": []}),
        )
        .await;
}

#[then("Pocket ID's record of that client lists that group as allowed")]
async fn group_allowed(w: &mut LiveWorld) {
    let stored = w
        .env
        .get_ok(&format!("/api/oidc/clients/{}", w.client_id()))
        .await;
    assert!(
        has_id(&stored["allowedUserGroups"], w.group_id()),
        "allowed groups on client: {}",
        stored["allowedUserGroups"]
    );
}

#[then("Pocket ID's record of that client lists no allowed groups")]
async fn no_groups_allowed(w: &mut LiveWorld) {
    let stored = w
        .env
        .get_ok(&format!("/api/oidc/clients/{}", w.client_id()))
        .await;
    assert_eq!(stored["allowedUserGroups"], json!([]));
}

// --- deletion ----------------------------------------------------------------

#[when("I delete that client")]
async fn delete_client(w: &mut LiveWorld) {
    w.mcp()
        .call("delete_oidc_client", json!({"client_id": w.client_id()}))
        .await;
}

#[then("Pocket ID no longer has that client")]
async fn client_gone(w: &mut LiveWorld) {
    let (status, body) = w
        .env
        .get(&format!("/api/oidc/clients/{}", w.client_id()))
        .await;
    assert_eq!(status, 404, "client still present after delete: {body}");
}

// --- API definitions and access ---------------------------------------------

#[given(expr = "an API definition {string} for resource {string}")]
async fn given_api_definition(w: &mut LiveWorld, name: String, resource: String) {
    let api = w
        .mcp()
        .call_json(
            "create_api_definition",
            json!({"name": w.expand(&name), "resource": w.expand(&resource)}),
        )
        .await;
    let id = str_of(&api, "id").to_string();
    w.cleanup.push(format!("/api/apis/{id}"));
    w.api_id = Some(id);
}

#[when("I set that API definition's permissions to:")]
async fn set_permissions(w: &mut LiveWorld, step: &Step) {
    let permissions: Vec<Value> = step
        .table()
        .expect("a | key | name | table")
        .rows
        .iter()
        .map(|row| json!({"key": row[0], "name": row[1]}))
        .collect();
    let updated = w
        .mcp()
        .call_json(
            "set_api_definition_permissions",
            json!({"api_id": w.api_id(), "permissions": permissions}),
        )
        .await;
    for p in updated["permissions"]
        .as_array()
        .expect("permissions array")
    {
        w.permission_ids
            .insert(str_of(p, "key").to_string(), str_of(p, "id").to_string());
    }
}

#[when(expr = "I grant that client user-delegated access to permission {string}")]
async fn grant_access(w: &mut LiveWorld, key: String) {
    let perm_id = w.permission_ids[&key].clone();
    w.mcp()
        .call(
            "update_client_api_access",
            json!({
                "client_id": w.client_id(),
                "client_permission_ids": [],
                "user_delegated_permission_ids": [perm_id],
            }),
        )
        .await;
}

#[then(expr = "Pocket ID's record of that API definition has permission {string}")]
async fn api_has_permission(w: &mut LiveWorld, key: String) {
    let stored = w.env.get_ok(&format!("/api/apis/{}", w.api_id())).await;
    let perms = stored["permissions"].as_array().expect("permissions array");
    assert!(
        perms
            .iter()
            .any(|p| p["key"] == key && p["id"] == w.permission_ids[&key]),
        "permissions in Pocket ID: {perms:?}"
    );
}

#[then(expr = "Pocket ID's API access for that client delegates permission {string}")]
async fn access_delegates(w: &mut LiveWorld, key: String) {
    let access = w
        .env
        .get_ok(&format!("/api/api-access/{}", w.client_id()))
        .await;
    assert_eq!(
        access["userDelegatedPermissionIds"],
        json!([w.permission_ids[&key]])
    );
    assert_eq!(access["clientPermissionIds"], json!([]));
}
