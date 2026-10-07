//! CrabCache: a Redis-compatible (RESP2) in-memory cache server.

pub mod commands;
pub mod config;
pub mod protocol;
pub mod server;
pub mod store;
pub mod util;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
