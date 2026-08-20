//! Steps about the MCP server itself: tiers, tool availability, error
//! mapping, and startup validation.

use std::collections::BTreeSet;
use std::process::Stdio;

use cucumber::{given, then, when};
use pocket_id_mcp::tools::{CATALOG, Tier};
use serde_json::json;

use crate::common::{Mode, server_command};
use crate::world::LiveWorld;

#[given("an MCP server with default tiers")]
async fn default_server(w: &mut LiveWorld) {
    w.spawn(Mode::DEFAULT).await;
}

#[given("an MCP server with the dangerous tier enabled")]
async fn dangerous_server(w: &mut LiveWorld) {
    w.spawn(Mode::DANGEROUS).await;
}

#[given("a read-only MCP server")]
async fn read_only_server(w: &mut LiveWorld) {
    w.spawn(Mode::READ_ONLY).await;
}

// --- tool availability -----------------------------------------------------

#[then(expr = "the server does not offer {string}")]
async fn does_not_offer(w: &mut LiveWorld, tool: String) {
    let names = w.mcp().tool_names().await;
    assert!(!names.contains(&tool), "{tool} is advertised: {names:?}");
}

#[then("the server offers exactly the read-tier tools")]
async fn offers_read_tier(w: &mut LiveWorld) {
    let read: BTreeSet<String> = CATALOG
        .iter()
        .filter(|t| t.tier == Tier::Read)
        .map(|t| t.name.to_string())
        .collect();
    assert_eq!(w.mcp().tool_names().await, read);
}

/// Unregistered tools are refused at the protocol level — nothing reaches
/// Pocket ID.
#[then(expr = "calling {string} with username {string} is refused by the protocol")]
async fn call_refused_username(w: &mut LiveWorld, tool: String, username: String) {
    let username = w.expand(&username);
    let attempt = w
        .mcp()
        .try_call(
            &tool,
            json!({"username": username, "email": format!("{username}@example.com")}),
        )
        .await;
    assert!(attempt.is_err(), "{tool} was callable: {attempt:?}");
}

// --- error mapping -----------------------------------------------------------

#[when(expr = "I call {string} for user id {string}")]
async fn call_for_user_id(w: &mut LiveWorld, tool: String, user_id: String) {
    w.last_error = Some(w.mcp().call_err(&tool, json!({"user_id": user_id})).await);
}

#[then(expr = "the tool fails with status {int} and {string}")]
async fn tool_fails_with(w: &mut LiveWorld, status: u16, needle: String) {
    let text = w.last_error();
    assert!(text.contains(&status.to_string()), "error text: {text}");
    assert!(
        text.to_lowercase().contains(&needle.to_lowercase()),
        "error text: {text}"
    );
}

// --- startup validation ------------------------------------------------------

async fn run_server(w: &mut LiveWorld, base_url: &str, api_key: &str) {
    let out = server_command(base_url, api_key, Mode::DEFAULT)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .expect("run binary");
    w.process = Some(out);
}

#[when(expr = "the server is started with API key {string}")]
async fn start_with_key(w: &mut LiveWorld, api_key: String) {
    let base_url = w.env.base_url.clone();
    run_server(w, &base_url, &api_key).await;
}

#[when(expr = "the server is started against {string}")]
async fn start_against(w: &mut LiveWorld, base_url: String) {
    run_server(w, &base_url, "irrelevant").await;
}

#[then(expr = "it exits with an error mentioning {string}")]
async fn exits_with_error(w: &mut LiveWorld, needle: String) {
    let out = w.process.as_ref().expect("a started server");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "server started: {stderr}");
    assert!(stderr.contains(&needle), "stderr: {stderr}");
}
