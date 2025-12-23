//! Shard implementation and management

pub mod manager;
pub mod optimized_manager;
pub mod eviction_manager;
pub mod wal_manager;
pub mod shard;

pub use manager::ShardManager;
pub use optimized_manager::{OptimizedShardManager, OptimizedShard, OptimizedShardStats};
pub use eviction_manager::{EvictionShardManager, EvictionShard};
pub use wal_manager::WALShardManager;
pub use shard::Shard;