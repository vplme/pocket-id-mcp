//! Step definitions, one module per area. `Given`/`When` act through tools;
//! `Then` reads Pocket ID back over REST (or proves a write-only value by
//! using it).

mod admin;
mod groups;
mod oidc;
mod server;
mod users;
