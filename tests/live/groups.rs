//! User-group tools, verified by reading groups back from Pocket ID over REST.

use serde_json::json;

use crate::common::{LiveEnv, Mcp, Mode, has_id, str_of, unique};

async fn create_group(mcp: &Mcp, name: &str) -> String {
    let group = mcp
        .call_json(
            "create_group",
            json!({"name": name, "friendlyName": "Live Group"}),
        )
        .await;
    str_of(&group, "id").to_string()
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn create_group_appears_in_pocket_id() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let name = unique("live-group");
    let id = create_group(&mcp, &name).await;

    let stored = env.get_ok(&format!("/api/user-groups/{id}")).await;
    assert_eq!(stored["name"], name);
    assert_eq!(stored["friendlyName"], "Live Group");

    let listed = env.get_ok(&format!("/api/user-groups?search={name}")).await;
    assert!(has_id(&listed, &id), "group listed by Pocket ID: {listed}");

    env.cleanup(&[format!("/api/user-groups/{id}")]).await;
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn update_group_persists() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let name = unique("live-group-upd");
    let id = create_group(&mcp, &name).await;

    let renamed = format!("{name}-renamed");
    mcp.call(
        "update_group",
        json!({"group_id": id, "name": renamed, "friendlyName": "Renamed Group"}),
    )
    .await;

    let stored = env.get_ok(&format!("/api/user-groups/{id}")).await;
    assert_eq!(stored["name"], renamed);
    assert_eq!(stored["friendlyName"], "Renamed Group");

    env.cleanup(&[format!("/api/user-groups/{id}")]).await;
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn set_group_users_persists() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let group_id = create_group(&mcp, &unique("live-group-users")).await;
    let username = unique("live-grouped");
    let user = mcp
        .call_json(
            "create_user",
            json!({"username": username, "email": format!("{username}@example.com")}),
        )
        .await;
    let user_id = str_of(&user, "id").to_string();

    mcp.call(
        "set_group_users",
        json!({"group_id": group_id, "user_ids": [user_id]}),
    )
    .await;

    let stored = env.get_ok(&format!("/api/user-groups/{group_id}")).await;
    assert!(
        has_id(&stored["users"], &user_id),
        "members: {}",
        stored["users"]
    );
    let user_groups = env.get_ok(&format!("/api/users/{user_id}/groups")).await;
    assert!(
        has_id(&user_groups, &group_id),
        "user's groups: {user_groups}"
    );

    // Full replacement: an empty list removes the member again.
    mcp.call(
        "set_group_users",
        json!({"group_id": group_id, "user_ids": []}),
    )
    .await;
    let stored = env.get_ok(&format!("/api/user-groups/{group_id}")).await;
    assert!(
        !has_id(&stored["users"], &user_id),
        "members after clear: {}",
        stored["users"]
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
async fn group_custom_claims_persist() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let group_id = create_group(&mcp, &unique("live-group-claims")).await;

    mcp.call(
        "update_group_custom_claims",
        json!({"group_id": group_id, "claims": [{"key": "cost_center", "value": "4711"}]}),
    )
    .await;

    let stored = env.get_ok(&format!("/api/user-groups/{group_id}")).await;
    let claims = stored["customClaims"]
        .as_array()
        .expect("customClaims array");
    assert!(
        claims
            .iter()
            .any(|c| c["key"] == "cost_center" && c["value"] == "4711"),
        "claims: {claims:?}"
    );

    env.cleanup(&[format!("/api/user-groups/{group_id}")]).await;
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn delete_group_removes_it() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let id = create_group(&mcp, &unique("live-group-del")).await;

    mcp.call("delete_group", json!({"group_id": id})).await;

    let (status, body) = env.get(&format!("/api/user-groups/{id}")).await;
    assert_eq!(status, 404, "group still present after delete: {body}");
    mcp.shutdown().await;
}
