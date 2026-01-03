//! Ultra-fast optimizations for achieving 500k ops/sec with P99 < 10ms

pub mod arena_allocator;
pub mod assembly_optimized;
pub mod custom_lockfree_map;
pub mod hybrid_server;
pub mod lockfree_shard_manager;
pub mod lockfree_store;
pub mod response_pool;
pub mod simd_parser;
pub mod simple_server;
pub mod toon_hybrid_server;
pub mod ultra_server;
pub mod zero_copy_parser;

// Sprint 3 & 4 optimizations
pub mod arm64_simd;
pub mod cpu_optimized;
pub mod io_uring_server;
pub mod toon_ultimate_server;
pub mod ultimate_server;

pub use arena_allocator::ArenaAllocator;
pub use assembly_optimized::*;
pub use custom_lockfree_map::{CustomLockFreeMap, CustomLockFreeMapStats};
pub use hybrid_server::HybridServer;
pub use lockfree_shard_manager::{LockFreeShardManager, LockFreeShardManagerStatsSnapshot};
pub use lockfree_store::{LockFreeStore, LockFreeStoreStatsSnapshot};
pub use response_pool::ResponsePool;
pub use simd_parser::{parse_batch_simd, parse_command_simd};
pub use simple_server::SimpleServer;
pub use toon_hybrid_server::ToonHybridServer;
pub use ultra_server::UltraFastServer;
pub use zero_copy_parser::{CommandRef, ZeroCopyParser};

// Sprint 3 & 4 exports
pub use arm64_simd::{hash_key_neon, parse_command_neon};
pub use cpu_optimized::*;
pub use io_uring_server::IoUringServer;
pub use toon_ultimate_server::ToonUltimateServer;
pub use ultimate_server::UltimateServer;
