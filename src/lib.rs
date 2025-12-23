//! CrabCache - Predictable, Memory-Efficient Cache Engine
//!
//! A modern cache system written in Rust, designed to be more predictable
//! than Redis and Dragonfly, with better memory efficiency and true multi-core support.

pub mod client;
pub mod config;
pub mod metrics;
pub mod protocol;
pub mod router;
pub mod security;
pub mod server;
pub mod shard;
pub mod store;
pub mod eviction;
pub mod ttl;
pub mod wal;
pub mod utils;

pub use config::Config;

/// CrabCache version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Result type alias for CrabCache operations
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;