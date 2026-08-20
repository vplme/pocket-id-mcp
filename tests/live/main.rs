//! Live integration suite: Gherkin features under `tests/features/`, run by
//! cucumber against a real Pocket ID. Every scenario drives the real
//! `pocket-id-mcp` binary over MCP (stdio) and verifies the effect
//! independently through Pocket ID's REST API — never through our own client.
//!
//! Opt-in (a `harness = false` binary has no `#[ignore]`), so plain
//! `cargo test` stays hermetic:
//!
//! ```sh
//! POCKET_ID_LIVE=1 cargo test --test live
//! POCKET_ID_LIVE=1 cargo test --test live -- --tags @oidc        # one area
//! POCKET_ID_LIVE=1 cargo test --test live -- --name "secret"     # by scenario name
//! ```
//!
//! Needs Docker (a pinned Pocket ID container is started and bootstrapped
//! automatically), or an existing instance via `POCKET_ID_LIVE_URL` +
//! `POCKET_ID_LIVE_API_KEY`; scenarios tagged `@needs-bootstrap` are then
//! skipped. See `common.rs` for all knobs.

mod common;
mod steps;
mod world;

use cucumber::World as _;

use crate::world::LiveWorld;

#[tokio::main]
async fn main() {
    if std::env::var_os("POCKET_ID_LIVE").is_none() {
        eprintln!(
            "live suite skipped: set POCKET_ID_LIVE=1 (needs Docker or POCKET_ID_LIVE_URL + POCKET_ID_LIVE_API_KEY)"
        );
        return;
    }
    // A user-supplied instance has no admin session, so scenarios that need
    // bootstrap-only material (a spare API key) cannot run there.
    let user_supplied = std::env::var_os("POCKET_ID_LIVE_URL").is_some();
    LiveWorld::cucumber()
        .max_concurrent_scenarios(8)
        .fail_on_skipped()
        .after(|_feature, _rule, _scenario, _finished, world| {
            Box::pin(async move {
                if let Some(w) = world {
                    w.teardown().await;
                }
            })
        })
        .filter_run_and_exit("tests/features", move |_, _, scenario| {
            !(user_supplied && scenario.tags.iter().any(|t| t == "needs-bootstrap"))
        })
        .await;
}
