//! Step definitions, one module per area. `Given`/`When` act through tools;
//! `Then` reads Pocket ID back over REST (or proves a write-only value by
//! using it). `readback` holds the subject-generic steps shared by all areas.

mod admin;
mod groups;
mod oidc;
mod readback;
mod server;
mod users;
