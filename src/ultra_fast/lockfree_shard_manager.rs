//! Lock-free shard manager for ultra-low latency operations

use crate::metrics::SharedMetrics;
use crate::protocol::{Command, Response};
use crate::ultra_fast::{
    lockfree_store::{LockFreeStore, LockFreeStoreStatsSnapshot},
    zero_copy_parser::CommandRef,
};
use ahash::AHasher;
use bytes::Bytes;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Ultra-fast lock-free shard manager
pub struct LockFreeShardManager {
    shards: Vec<Arc<LockFreeStore>>,
    shard_mask: usize,
    metrics: SharedMetrics,
    stats: LockFreeShardManagerStats,
}

/// Lock-free shard manager statistics
#[derive(Debug)]
pub struct LockFreeShardManagerStats {
    pub total_operations: AtomicU64,
    pub total_hits: AtomicU64,
    pub total_misses: AtomicU64,
    pub shard_routing_time_ns: AtomicU64,
}

impl LockFreeShardManager {
    /// Create new lock-free shard manager
    pub fn new(num_shards: usize, max_memory_per_shard: usize) -> Self {
        let shard_count = num_shards.next_power_of_two();
        let shard_mask = shard_count - 1;

        let shards: Vec<Arc<LockFreeStore>> = (0..shard_count)
            .map(|_| Arc::new(LockFreeStore::new(max_memory_per_shard)))
            .collect();

        let metrics = crate::metrics::create_shared_metrics(shard_count);

        tracing::info!(
            "LockFreeShardManager created: {} shards, {}B per shard, lock-free architecture",
            shard_count,
            max_memory_per_shard
        );

        Self {
            shards,
            shard_mask,
            metrics,
            stats: LockFreeShardManagerStats::new(),
        }
    }

    /// Get shard for key using ultra-fast hashing
    #[inline(always)]
    fn get_shard_for_key(&self, key: &[u8]) -> &Arc<LockFreeStore> {
        let hash = self.hash_key_ultra_fast(key);
        let shard_index = (hash as usize) & self.shard_mask;
        &self.shards[shard_index]
    }

    /// Ultra-fast key hashing
    #[inline(always)]
    fn hash_key_ultra_fast(&self, key: &[u8]) -> u64 {
        let mut hasher = AHasher::default();
        key.hash(&mut hasher);
        hasher.finish()
    }

    /// Process command with zero-copy optimization
    #[inline(always)]
    pub async fn process_command_zero_copy(&self, command_ref: CommandRef<'_>) -> Response {
        let start_time = std::time::Instant::now();
        self.stats.total_operations.fetch_add(1, Ordering::Relaxed);

        let response = match command_ref {
            CommandRef::Ping => Response::Pong,

            CommandRef::Get { key } => {
                let shard = self.get_shard_for_key(key);
                let key_bytes = Bytes::copy_from_slice(key);

                match shard.get(&key_bytes) {
                    Some(value) => {
                        self.stats.total_hits.fetch_add(1, Ordering::Relaxed);
                        Response::Value(value)
                    }
                    None => {
                        self.stats.total_misses.fetch_add(1, Ordering::Relaxed);
                        Response::Null
                    }
                }
            }

            CommandRef::Put { key, value, ttl } => {
                let shard = self.get_shard_for_key(key);
                let key_bytes = Bytes::copy_from_slice(key);
                let value_bytes = Bytes::copy_from_slice(value);

                if shard.put(key_bytes, value_bytes, ttl) {
                    Response::Ok
                } else {
                    Response::Error("Failed to store value".to_string())
                }
            }

            CommandRef::Del { key } => {
                let shard = self.get_shard_for_key(key);
                let key_bytes = Bytes::copy_from_slice(key);

                if shard.delete(&key_bytes) {
                    Response::Ok
                } else {
                    Response::Null
                }
            }

            CommandRef::Expire { key, ttl } => {
                let shard = self.get_shard_for_key(key);
                let key_bytes = Bytes::copy_from_slice(key);

                if shard.expire(&key_bytes, ttl) {
                    Response::Ok
                } else {
                    Response::Null
                }
            }

            CommandRef::Stats => {
                let stats = self.get_detailed_stats().await;
                Response::Stats(stats)
            }

            CommandRef::Metrics => {
                let metrics = self.get_performance_metrics().await;
                Response::Stats(metrics)
            }
        };

        // Record routing time
        let routing_time = start_time.elapsed().as_nanos() as u64;
        self.stats
            .shard_routing_time_ns
            .fetch_add(routing_time, Ordering::Relaxed);

        response
    }

    /// Process regular command (for compatibility)
    pub async fn process_command(&self, command: Command) -> Response {
        self.stats.total_operations.fetch_add(1, Ordering::Relaxed);

        match command {
            Command::Ping => Response::Pong,

            Command::Get { key } => {
                let shard = self.get_shard_for_key(&key);

                match shard.get(&key) {
                    Some(value) => {
                        self.stats.total_hits.fetch_add(1, Ordering::Relaxed);
                        Response::Value(value)
                    }
                    None => {
                        self.stats.total_misses.fetch_add(1, Ordering::Relaxed);
                        Response::Null
                    }
                }
            }

            Command::Put { key, value, ttl } => {
                let shard = self.get_shard_for_key(&key);

                if shard.put(key, value, ttl) {
                    Response::Ok
                } else {
                    Response::Error("Failed to store value".to_string())
                }
            }

            Command::Del { key } => {
                let shard = self.get_shard_for_key(&key);

                if shard.delete(&key) {
                    Response::Ok
                } else {
                    Response::Null
                }
            }

            Command::Expire { key, ttl } => {
                let shard = self.get_shard_for_key(&key);

                if shard.expire(&key, ttl) {
                    Response::Ok
                } else {
                    Response::Null
                }
            }

            Command::Stats => {
                let stats = self.get_detailed_stats().await;
                Response::Stats(stats)
            }

            Command::Metrics => {
                let metrics = self.get_performance_metrics().await;
                Response::Stats(metrics)
            }
        }
    }

    /// Get detailed statistics
    async fn get_detailed_stats(&self) -> String {
        let mut all_stats = Vec::new();
        let mut total_entries = 0;
        let mut total_memory = 0;
        let mut total_hits = 0;
        let mut total_misses = 0;

        for (i, shard) in self.shards.iter().enumerate() {
            let stats = shard.get_stats();
            total_entries += stats.data_map_len;
            total_memory += stats.memory_used;
            total_hits += stats.hits;
            total_misses += stats.misses;

            all_stats.push(format!(
                "shard_{}: entries={}, memory={}KB, hit_ratio={:.2}%",
                i,
                stats.data_map_len,
                stats.memory_used / 1024,
                if stats.hits + stats.misses > 0 {
                    (stats.hits as f64 / (stats.hits + stats.misses) as f64) * 100.0
                } else {
                    0.0
                }
            ));
        }

        let overall_hit_ratio = if total_hits + total_misses > 0 {
            (total_hits as f64 / (total_hits + total_misses) as f64) * 100.0
        } else {
            0.0
        };

        let avg_routing_time = if self.stats.total_operations.load(Ordering::Relaxed) > 0 {
            self.stats.shard_routing_time_ns.load(Ordering::Relaxed) as f64
                / self.stats.total_operations.load(Ordering::Relaxed) as f64
        } else {
            0.0
        };

        format!(
            "LockFreeShardManager Stats:\n\
            Total Entries: {}\n\
            Total Memory: {}MB\n\
            Overall Hit Ratio: {:.2}%\n\
            Total Operations: {}\n\
            Average Routing Time: {:.2}ns\n\
            Shards: {}\n\
            {}",
            total_entries,
            total_memory / (1024 * 1024),
            overall_hit_ratio,
            self.stats.total_operations.load(Ordering::Relaxed),
            avg_routing_time,
            self.shards.len(),
            all_stats.join("\n")
        )
    }

    /// Get performance metrics
    async fn get_performance_metrics(&self) -> String {
        let total_ops = self.stats.total_operations.load(Ordering::Relaxed);
        let total_hits = self.stats.total_hits.load(Ordering::Relaxed);
        let total_misses = self.stats.total_misses.load(Ordering::Relaxed);
        let avg_routing_time = if total_ops > 0 {
            self.stats.shard_routing_time_ns.load(Ordering::Relaxed) as f64 / total_ops as f64
        } else {
            0.0
        };

        format!(
            "ops_total={} hits={} misses={} hit_ratio={:.3} avg_routing_time_ns={:.2} shards={}",
            total_ops,
            total_hits,
            total_misses,
            if total_ops > 0 {
                total_hits as f64 / total_ops as f64
            } else {
                0.0
            },
            avg_routing_time,
            self.shards.len()
        )
    }

    /// Get shared metrics for external use
    pub fn get_shared_metrics(&self) -> SharedMetrics {
        Arc::clone(&self.metrics)
    }

    /// Get current statistics snapshot
    pub fn get_stats_snapshot(&self) -> LockFreeShardManagerStatsSnapshot {
        let mut shard_stats = Vec::new();
        for shard in &self.shards {
            shard_stats.push(shard.get_stats());
        }

        LockFreeShardManagerStatsSnapshot {
            total_operations: self.stats.total_operations.load(Ordering::Relaxed),
            total_hits: self.stats.total_hits.load(Ordering::Relaxed),
            total_misses: self.stats.total_misses.load(Ordering::Relaxed),
            avg_routing_time_ns: if self.stats.total_operations.load(Ordering::Relaxed) > 0 {
                self.stats.shard_routing_time_ns.load(Ordering::Relaxed) as f64
                    / self.stats.total_operations.load(Ordering::Relaxed) as f64
            } else {
                0.0
            },
            shard_count: self.shards.len(),
            shard_stats,
        }
    }
}

impl LockFreeShardManagerStats {
    fn new() -> Self {
        Self {
            total_operations: AtomicU64::new(0),
            total_hits: AtomicU64::new(0),
            total_misses: AtomicU64::new(0),
            shard_routing_time_ns: AtomicU64::new(0),
        }
    }
}

/// Snapshot of lock-free shard manager statistics
#[derive(Debug, Clone)]
pub struct LockFreeShardManagerStatsSnapshot {
    pub total_operations: u64,
    pub total_hits: u64,
    pub total_misses: u64,
    pub avg_routing_time_ns: f64,
    pub shard_count: usize,
    pub shard_stats: Vec<LockFreeStoreStatsSnapshot>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lockfree_shard_manager() {
        let manager = LockFreeShardManager::new(4, 1024 * 1024);

        // Test zero-copy operations
        let get_cmd = CommandRef::Get { key: b"test_key" };
        let response = manager.process_command_zero_copy(get_cmd).await;
        assert!(matches!(response, Response::Null));

        let put_cmd = CommandRef::Put {
            key: b"test_key",
            value: b"test_value",
            ttl: None,
        };
        let response = manager.process_command_zero_copy(put_cmd).await;
        assert!(matches!(response, Response::Ok));

        let get_cmd = CommandRef::Get { key: b"test_key" };
        let response = manager.process_command_zero_copy(get_cmd).await;
        match response {
            Response::Value(value) => {
                assert_eq!(value, b"test_value");
            }
            _ => panic!("Expected value response"),
        }

        // Test regular operations
        let cmd = Command::Get {
            key: Bytes::from("test_key"),
        };
        let response = manager.process_command(cmd).await;
        match response {
            Response::Value(value) => {
                assert_eq!(value, b"test_value");
            }
            _ => panic!("Expected value response"),
        }
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        use std::sync::Arc;
        use tokio::task;

        let manager = Arc::new(LockFreeShardManager::new(8, 1024 * 1024));
        let mut handles = vec![];

        // Spawn multiple tasks
        for i in 0..10 {
            let manager_clone = Arc::clone(&manager);
            let handle = task::spawn(async move {
                for j in 0..100 {
                    let key = format!("key_{}_{}", i, j);
                    let value = format!("value_{}_{}", i, j);

                    let put_cmd = Command::Put {
                        key: Bytes::from(key.clone()),
                        value: Bytes::from(value.clone()),
                        ttl: None,
                    };
                    let response = manager_clone.process_command(put_cmd).await;
                    assert!(matches!(response, Response::Ok));

                    let get_cmd = Command::Get {
                        key: Bytes::from(key),
                    };
                    let response = manager_clone.process_command(get_cmd).await;
                    match response {
                        Response::Value(returned_value) => {
                            assert_eq!(returned_value, value.as_bytes());
                        }
                        _ => panic!("Expected value response"),
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        let stats = manager.get_stats_snapshot();
        println!("Final stats: {:?}", stats);
    }

    #[tokio::test]
    async fn test_performance() {
        let manager = LockFreeShardManager::new(4, 10 * 1024 * 1024);
        let iterations = 1000;

        // Benchmark zero-copy operations
        let start = std::time::Instant::now();
        for i in 0..iterations {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);

            let put_cmd = CommandRef::Put {
                key: key.as_bytes(),
                value: value.as_bytes(),
                ttl: None,
            };
            let response = manager.process_command_zero_copy(put_cmd).await;
            assert!(matches!(response, Response::Ok));
        }
        let put_time = start.elapsed();

        let start = std::time::Instant::now();
        for i in 0..iterations {
            let key = format!("key_{}", i);
            let get_cmd = CommandRef::Get {
                key: key.as_bytes(),
            };
            let response = manager.process_command_zero_copy(get_cmd).await;
            assert!(matches!(response, Response::Value(_)));
        }
        let get_time = start.elapsed();

        println!(
            "Zero-copy PUT time: {:?} ({:.2} ns/op)",
            put_time,
            put_time.as_nanos() as f64 / iterations as f64
        );
        println!(
            "Zero-copy GET time: {:?} ({:.2} ns/op)",
            get_time,
            get_time.as_nanos() as f64 / iterations as f64
        );

        let stats = manager.get_stats_snapshot();
        println!(
            "Performance stats: avg_routing_time={:.2}ns",
            stats.avg_routing_time_ns
        );

        // Should be very fast
        assert!(get_time.as_nanos() / iterations < 1000); // Under 1μs per operation
    }
}
