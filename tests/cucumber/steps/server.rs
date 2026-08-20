//! Steps about the MCP server itself (tiers, tool availability).

use cucumber::given;

use crate::common::Mode;
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
