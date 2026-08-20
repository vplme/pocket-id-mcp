//! Server-level behaviour that only shows against a real upstream: tier
//! gating over the wire, upstream error mapping, and startup validation.

use std::collections::BTreeSet;
use std::process::Stdio;

use pocket_id_mcp::tools::{CATALOG, Tier};
use serde_json::json;

use crate::common::{LiveEnv, Mcp, Mode, server_command};

fn catalog_names(tiers: &[Tier]) -> BTreeSet<String> {
    CATALOG
        .iter()
        .filter(|t| tiers.contains(&t.tier))
        .map(|t| t.name.to_string())
        .collect()
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn read_only_mode_hides_and_refuses_write_tools() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::READ_ONLY).await;

    let advertised = mcp.tool_names().await;
    assert_eq!(advertised, catalog_names(&[Tier::Read]));
    let mutating = catalog_names(&[Tier::Write, Tier::Dangerous]);
    assert!(advertised.is_disjoint(&mutating));

    // Refused at the protocol level — nothing reaches Pocket ID.
    let attempt = mcp
        .try_call(
            "create_user",
            json!({"username": "live-should-not-exist", "email": "no@example.com"}),
        )
        .await;
    assert!(attempt.is_err(), "write tool callable in read-only mode");
    let listed = env.get_ok("/api/users?search=live-should-not-exist").await;
    assert_eq!(listed["data"], json!([]));
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn upstream_error_is_a_tool_error_with_status() {
    let env = LiveEnv::acquire().await;
    let mcp = Mcp::spawn(env, Mode::DEFAULT).await;
    let text = mcp
        .call_err("get_user", json!({"user_id": "does-not-exist"}))
        .await;
    assert!(text.contains("404"), "error text: {text}");
    assert!(
        text.to_lowercase().contains("not found"),
        "error text: {text}"
    );
    mcp.shutdown().await;
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn startup_rejects_bad_api_key() {
    let env = LiveEnv::acquire().await;
    let out = server_command(&env.base_url, "not-a-valid-key", Mode::DEFAULT)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .expect("run binary");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "server started with a bad key: {stderr}"
    );
    assert!(stderr.contains("API key rejected"), "stderr: {stderr}");
}

#[tokio::test]
#[ignore = "live: needs Docker or POCKET_ID_LIVE_URL; run with --ignored"]
async fn startup_rejects_unreachable_instance() {
    // Port 9 (discard) is closed on any sane host.
    let out = server_command("http://127.0.0.1:9", "irrelevant", Mode::DEFAULT)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .expect("run binary");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "server started without upstream: {stderr}"
    );
    assert!(
        stderr.contains("cannot reach Pocket ID"),
        "stderr: {stderr}"
    );
}
