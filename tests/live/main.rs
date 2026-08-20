//! Live integration suite: drives the real `pocket-id-mcp` binary over MCP
//! (stdio) against a real Pocket ID instance, and verifies every mutation
//! independently through Pocket ID's REST API — not through our own client.
//!
//! Opt-in: every test is `#[ignore]`d so plain `cargo test` stays hermetic.
//! Run with:
//!
//! ```sh
//! cargo test --test live -- --ignored
//! ```
//!
//! Needs Docker (a pinned Pocket ID container is started and bootstrapped
//! automatically), or an existing instance via `POCKET_ID_LIVE_URL` +
//! `POCKET_ID_LIVE_API_KEY`. See `common.rs` for all knobs.

mod common;

mod admin;
mod groups;
mod oidc;
mod server;
mod users;
