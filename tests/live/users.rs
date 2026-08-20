//! User tools: create/update/group membership/custom claims/delete, each
//! verified by reading the user back from Pocket ID over REST.

use serde_json::json;

use crate::common::{LiveEnv, Mcp, Mode, has_id, str_of, unique};

fn user_input(username: &str) -> serde_json::Value {
    json!({
        "username": username,
        "email": format!("{username}@example.com"),
        "firstName": "Live",
        "lastName": "Tester",
    })
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn create_user_appears_in_pocket_id() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let username = unique("live-user");

    let created = mcp.call_json("create_user", user_input(&username)).await;
    let id = str_of(&created, "id").to_string();

    let stored = env.get_ok(&format!("/api/users/{id}")).await;
    assert_eq!(stored["username"], username);
    assert_eq!(stored["email"], format!("{username}@example.com"));
    assert_eq!(stored["firstName"], "Live");
    assert_eq!(stored["lastName"], "Tester");
    assert_eq!(stored["isAdmin"], false);

    let listed = env.get_ok(&format!("/api/users?search={username}")).await;
    assert!(has_id(&listed, &id), "user listed by Pocket ID: {listed}");

    env.cleanup(&[format!("/api/users/{id}")]).await;
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn update_user_persists() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let username = unique("live-upd-user");
    let created = mcp.call_json("create_user", user_input(&username)).await;
    let id = str_of(&created, "id").to_string();

    let new_email = format!("{username}-new@example.com");
    mcp.call(
        "update_user",
        json!({
            "user_id": id,
            "username": username,
            "email": new_email,
            "firstName": "Updated",
            "lastName": "Person",
            "disabled": true,
        }),
    )
    .await;

    let stored = env.get_ok(&format!("/api/users/{id}")).await;
    assert_eq!(stored["email"], new_email);
    assert_eq!(stored["firstName"], "Updated");
    assert_eq!(stored["lastName"], "Person");
    assert_eq!(stored["disabled"], true);

    env.cleanup(&[format!("/api/users/{id}")]).await;
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn set_user_groups_persists() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let user = mcp
        .call_json("create_user", user_input(&unique("live-member")))
        .await;
    let user_id = str_of(&user, "id").to_string();
    let group = mcp
        .call_json(
            "create_group",
            json!({"name": unique("live-membership"), "friendlyName": "Live Membership"}),
        )
        .await;
    let group_id = str_of(&group, "id").to_string();

    mcp.call(
        "set_user_groups",
        json!({"user_id": user_id, "user_group_ids": [group_id]}),
    )
    .await;

    let groups = env.get_ok(&format!("/api/users/{user_id}/groups")).await;
    assert!(
        has_id(&groups, &group_id),
        "user's groups in Pocket ID: {groups}"
    );
    let stored_group = env.get_ok(&format!("/api/user-groups/{group_id}")).await;
    assert!(
        has_id(&stored_group["users"], &user_id),
        "group members in Pocket ID: {}",
        stored_group["users"]
    );

    env.cleanup(&[
        format!("/api/users/{user_id}"),
        format!("/api/user-groups/{group_id}"),
    ])
    .await;
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn user_custom_claims_persist() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let user = mcp
        .call_json("create_user", user_input(&unique("live-claims")))
        .await;
    let user_id = str_of(&user, "id").to_string();

    mcp.call(
        "update_user_custom_claims",
        json!({
            "user_id": user_id,
            "claims": [{"key": "department", "value": "qa"}, {"key": "tier", "value": "gold"}],
        }),
    )
    .await;

    let stored = env.get_ok(&format!("/api/users/{user_id}")).await;
    let claims: Vec<(String, String)> = stored["customClaims"]
        .as_array()
        .expect("customClaims array")
        .iter()
        .map(|c| (str_of(c, "key").to_string(), str_of(c, "value").to_string()))
        .collect();
    assert!(
        claims.contains(&("department".into(), "qa".into())),
        "claims: {claims:?}"
    );
    assert!(
        claims.contains(&("tier".into(), "gold".into())),
        "claims: {claims:?}"
    );

    env.cleanup(&[format!("/api/users/{user_id}")]).await;
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn delete_user_requires_dangerous_tier() {
    let env = LiveEnv::acquire().await;
    let default_mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let user = default_mcp
        .call_json("create_user", user_input(&unique("live-doomed")))
        .await;
    let user_id = str_of(&user, "id").to_string();

    // Default tiers: the tool is not even registered, and calling it is a
    // protocol-level error — the user survives.
    assert!(!default_mcp.tool_names().await.contains("delete_user"));
    let refused = default_mcp
        .try_call("delete_user", json!({"user_id": user_id}))
        .await;
    assert!(
        refused.is_err(),
        "delete_user must be unavailable by default"
    );
    assert!(
        env.get(&format!("/api/users/{user_id}"))
            .await
            .0
            .is_success()
    );
    default_mcp.shutdown().await;

    // Dangerous tier opted in: the user really disappears from Pocket ID.
    let dangerous_mcp = Mcp::spawn(env, Mode::DANGEROUS).await;
    dangerous_mcp
        .call("delete_user", json!({"user_id": user_id}))
        .await;
    let (status, body) = env.get(&format!("/api/users/{user_id}")).await;
    assert_eq!(status, 404, "user still present after delete: {body}");
    dangerous_mcp.shutdown().await;
}
