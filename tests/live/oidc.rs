//! OIDC client tools: every mutation made through a tool is verified by
//! reading Pocket ID back directly over REST.

use serde_json::json;

use crate::common::{LiveEnv, Mcp, Mode, has_id, str_of, unique};

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn create_oidc_client_appears_in_pocket_id() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let name = unique("live-app");
    let callbacks = json!([
        "https://app.example.com/callback",
        "https://app.example.com/alt-callback"
    ]);

    let created = mcp
        .call_json(
            "create_oidc_client",
            json!({
                "name": name,
                "description": "created by the live suite",
                "callbackURLs": callbacks,
                "pkceEnabled": true,
                "isPublic": false,
            }),
        )
        .await;
    let id = str_of(&created, "id").to_string();
    assert_eq!(created["name"], name, "tool response echoes the name");

    // Independent read-back: Pocket ID itself must now hold this client.
    let stored = env.get_ok(&format!("/api/oidc/clients/{id}")).await;
    assert_eq!(stored["name"], name);
    assert_eq!(stored["description"], "created by the live suite");
    assert_eq!(stored["callbackURLs"], callbacks);
    assert_eq!(stored["pkceEnabled"], true);
    assert_eq!(stored["isPublic"], false);

    let listed = env
        .get_ok(&format!("/api/oidc/clients?search={name}"))
        .await;
    assert!(has_id(&listed, &id), "client listed by Pocket ID: {listed}");

    env.cleanup(&[format!("/api/oidc/clients/{id}")]).await;
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn update_oidc_client_persists() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let name = unique("live-upd");
    let created = mcp
        .call_json(
            "create_oidc_client",
            json!({"name": name, "callbackURLs": ["https://old.example.com/cb"]}),
        )
        .await;
    let id = str_of(&created, "id").to_string();

    let renamed = format!("{name}-renamed");
    mcp.call(
        "update_oidc_client",
        json!({
            "client_id": id,
            "name": renamed,
            "callbackURLs": ["https://new.example.com/cb"],
            "logoutCallbackURLs": ["https://new.example.com/logout"],
            "skipConsent": true,
            "pkceEnabled": true,
            "isPublic": false,
        }),
    )
    .await;

    let stored = env.get_ok(&format!("/api/oidc/clients/{id}")).await;
    assert_eq!(stored["name"], renamed);
    assert_eq!(
        stored["callbackURLs"],
        json!(["https://new.example.com/cb"])
    );
    assert_eq!(
        stored["logoutCallbackURLs"],
        json!(["https://new.example.com/logout"])
    );
    assert_eq!(stored["skipConsent"], true);
    assert_eq!(stored["pkceEnabled"], true);

    env.cleanup(&[format!("/api/oidc/clients/{id}")]).await;
    mcp.shutdown().await;
}

/// The secret is write-only, so "Pocket ID has it" is proven by using it:
/// the token introspection endpoint authenticates the client with
/// `client_id:secret` (HTTP Basic) and answers 200 for valid credentials,
/// 401 otherwise — regardless of the token being introspected.
#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn client_secret_is_usable_and_rotation_invalidates_old() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let created = mcp
        .call_json(
            "create_oidc_client",
            json!({"name": unique("live-secret"), "callbackURLs": ["https://s.example.com/cb"]}),
        )
        .await;
    let id = str_of(&created, "id").to_string();

    let chosen = format!("{}-chosen-secret", unique("live"));
    let set = mcp
        .call_json(
            "create_oidc_client_secret",
            json!({"client_id": id, "secret": chosen}),
        )
        .await;
    assert_eq!(set["secret"], chosen, "chosen secret echoed back once");
    assert_eq!(introspect_status(env, &id, &chosen).await, 200);
    assert_eq!(
        introspect_status(env, &id, "definitely-not-the-secret").await,
        401
    );

    // Rotate to a generated secret: the old one must stop working.
    let rotated = mcp
        .call_json("create_oidc_client_secret", json!({"client_id": id}))
        .await;
    let generated = str_of(&rotated, "secret").to_string();
    assert!(
        generated.len() >= 16 && generated != chosen,
        "generated secret: {generated:?}"
    );
    assert_eq!(introspect_status(env, &id, &generated).await, 200);
    assert_eq!(introspect_status(env, &id, &chosen).await, 401);

    env.cleanup(&[format!("/api/oidc/clients/{id}")]).await;
    mcp.shutdown().await;
}

async fn introspect_status(env: &LiveEnv, client_id: &str, secret: &str) -> u16 {
    reqwest::Client::new()
        .post(env.url("/api/oidc/introspect"))
        .basic_auth(client_id, Some(secret))
        .form(&[("token", "not-a-real-token")])
        .send()
        .await
        .expect("introspect request")
        .status()
        .as_u16()
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn allowed_groups_restriction_visible_in_pocket_id() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let group = mcp
        .call_json(
            "create_group",
            json!({"name": unique("live-allowed"), "friendlyName": "Live Allowed"}),
        )
        .await;
    let group_id = str_of(&group, "id").to_string();
    let client = mcp
        .call_json(
            "create_oidc_client",
            json!({"name": unique("live-restricted"), "callbackURLs": ["https://r.example.com/cb"]}),
        )
        .await;
    let client_id = str_of(&client, "id").to_string();

    mcp.call(
        "update_oidc_client_allowed_groups",
        json!({"client_id": client_id, "user_group_ids": [group_id]}),
    )
    .await;
    let stored = env.get_ok(&format!("/api/oidc/clients/{client_id}")).await;
    assert!(
        has_id(&stored["allowedUserGroups"], &group_id),
        "allowed groups on client: {}",
        stored["allowedUserGroups"]
    );

    // Emptying the list lifts the restriction again.
    mcp.call(
        "update_oidc_client_allowed_groups",
        json!({"client_id": client_id, "user_group_ids": []}),
    )
    .await;
    let stored = env.get_ok(&format!("/api/oidc/clients/{client_id}")).await;
    assert_eq!(stored["allowedUserGroups"], json!([]));

    env.cleanup(&[
        format!("/api/oidc/clients/{client_id}"),
        format!("/api/user-groups/{group_id}"),
    ])
    .await;
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn delete_oidc_client_removes_it() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let created = mcp
        .call_json(
            "create_oidc_client",
            json!({"name": unique("live-del"), "callbackURLs": ["https://d.example.com/cb"]}),
        )
        .await;
    let id = str_of(&created, "id").to_string();
    assert!(
        env.get(&format!("/api/oidc/clients/{id}"))
            .await
            .0
            .is_success()
    );

    let result = mcp
        .call("delete_oidc_client", json!({"client_id": id}))
        .await;
    assert!(crate::common::text_of(&result).contains(&id));

    let (status, body) = env.get(&format!("/api/oidc/clients/{id}")).await;
    assert_eq!(status, 404, "client still present after delete: {body}");
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn api_definition_and_client_access_persist() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let client = mcp
        .call_json(
            "create_oidc_client",
            json!({"name": unique("live-api-client"), "isPublic": true, "pkceEnabled": true,
                   "callbackURLs": ["https://c.example.com/cb"]}),
        )
        .await;
    let client_id = str_of(&client, "id").to_string();

    let api_name = unique("live-api");
    let resource = format!("https://api.example.com/{api_name}");
    let api = mcp
        .call_json(
            "create_api_definition",
            json!({"name": api_name, "resource": resource}),
        )
        .await;
    let api_id = str_of(&api, "id").to_string();

    let with_perms = mcp
        .call_json(
            "set_api_definition_permissions",
            json!({"api_id": api_id, "permissions": [{"key": "read", "name": "Read things"}]}),
        )
        .await;
    let perm_id = with_perms["permissions"][0]["id"]
        .as_str()
        .expect("permission id")
        .to_string();

    let stored_api = env.get_ok(&format!("/api/apis/{api_id}")).await;
    assert_eq!(stored_api["name"], api_name);
    assert_eq!(stored_api["resource"], resource);
    assert_eq!(stored_api["permissions"][0]["key"], "read");
    assert_eq!(stored_api["permissions"][0]["id"], perm_id);

    mcp.call(
        "update_client_api_access",
        json!({
            "client_id": client_id,
            "client_permission_ids": [],
            "user_delegated_permission_ids": [perm_id],
        }),
    )
    .await;
    let access = env.get_ok(&format!("/api/api-access/{client_id}")).await;
    assert_eq!(access["userDelegatedPermissionIds"], json!([perm_id]));
    assert_eq!(access["clientPermissionIds"], json!([]));

    env.cleanup(&[
        format!("/api/apis/{api_id}"),
        format!("/api/oidc/clients/{client_id}"),
    ])
    .await;
    mcp.shutdown().await;
}
