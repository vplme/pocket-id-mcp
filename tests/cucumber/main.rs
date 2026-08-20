//! Cucumber spike for the live suite: the same harness as `tests/live`
//! (`common.rs` is shared), expressed as Gherkin features under
//! `tests/features/` with domain-language steps.
//!
//! Opt-in like the plain live suite (here via an env var, since a
//! `harness = false` binary has no `#[ignore]`):
//!
//! ```sh
//! POCKET_ID_LIVE=1 cargo test --test cucumber
//! POCKET_ID_LIVE=1 cargo test --test cucumber -- --tags @oidc   # subset
//! ```

// Shared with tests/live; this spike only exercises part of it.
#[path = "../live/common.rs"]
#[allow(dead_code)]
mod common;
mod steps;
mod world;

use cucumber::World as _;

use crate::world::LiveWorld;

#[tokio::main]
async fn main() {
    if std::env::var_os("POCKET_ID_LIVE").is_none() {
        eprintln!(
            "cucumber live suite skipped: set POCKET_ID_LIVE=1 (needs Docker or POCKET_ID_LIVE_URL)"
        );
        return;
    }
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
        .run_and_exit("tests/features")
        .await;
}
