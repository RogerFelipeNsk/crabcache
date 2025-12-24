//! Optimized shard manager with zero-copy and lock-free operations

use super::Shard;
use crate::metrics::SharedMetrics;
use crate::protocol::commands::{Command, Response};
use crate::store::{CrabCacheLockFreeMap, ZeroCopyStore};
use crate::utils::simd::SIMDParser;
use bytes::Bytes;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{debug, info, warn};

/// High-performance shard manager with extreme optimizations
pub struct OptimizedShardManager {
    shards: Vec<Arc<OptimizedShard>>,
    zero_copy_enabled: bool,
    simd_enabled: bool,
    lockfree_enabled: bool,
    metrics: SharedMetrics,
}

impl OptimizedShardManager {
    /// Create new optimized shard manager
    pub fn new(num_shards: usize, max_memory_per_shard: usize) -> Self {
        info!(
            "Creating OptimizedShardManager with {} shards, {}B per shard",
            num_shards, max_memory_per_shard
        );

        let metrics = crate::metrics::create_shared_metrics(num_shards);

        let shards: Vec<Arc<OptimizedShard>> = (0..num_shards)
            .map(|id| {
                Arc::new(OptimizedShard::new(
                    id,
                    max_memory_per_shard,
                    Arc::clone(&metrics),
                ))
            })
            .collect();

        // Start optimized cleanup tasks
        for shard in &shards {
            OptimizedShard::start_optimized_cleanup(Arc::clone(shard));
        }

        Self {
            shards,
            zero_copy_enabled: true,
            simd_enabled: true,
            lockfree_enabled: true,
            metrics,
        }
    }

    /// Get shard for key using SIMD-optimized hashing
    pub fn get_shard_for_key_simd(&self, key: &[u8]) -> &Arc<OptimizedShard> {
        let hash = if self.simd_enabled && key.len() >= 16 {
            SIMDParser::hash_key_simd(key)
        } else {
            crate::utils::hash::hash_key(key) as u64
        };

        let shard_index = (hash as usize) % self.shards.len();
        debug!(
            "SIMD Key {} -> shard {}",
            String::from_utf8_lossy(key),
            shard_index
        );
        &self.shards[shard_index]
    }

    /// Process command with extreme optimizations
    pub async fn process_command_optimized(&self, command: Command) -> Response {
        let start_time = Instant::now();

        let response = match &command {
            Command::Put { key, value, ttl } => {
                let shard = self.get_shard_for_key_simd(key);
                let shard_id = shard.id;

                let response = if self.zero_copy_enabled {
                    shard.put_zero_copy(key.clone(), value.clone(), *ttl).await
                } else {
                    shard.process_command(command).await
                };

                // Record metrics
                let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                let hit = matches!(response, Response::Ok);
                if let Ok(mut metrics) = self.metrics.try_write() {
                    metrics.record_operation(shard_id, "PUT", hit, latency_ms);
                }

                response
            }
            Command::Get { key } => {
                let shard = self.get_shard_for_key_simd(key);
                let shard_id = shard.id;

                let response = if self.zero_copy_enabled {
                    shard.get_zero_copy(key).await
                } else {
                    shard.process_command(command).await
                };

                // Record metrics
                let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                let hit = matches!(response, Response::Value(_));
                if let Ok(mut metrics) = self.metrics.try_write() {
                    metrics.record_operation(shard_id, "GET", hit, latency_ms);
                }

                response
            }
            Command::Del { key } => {
                let shard = self.get_shard_for_key_simd(key);
                let shard_id = shard.id;

                let response = if self.zero_copy_enabled {
                    shard.del_zero_copy(key).await
                } else {
                    shard.process_command(command).await
                };

                // Record metrics
                let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                let hit = matches!(response, Response::Ok);
                if let Ok(mut metrics) = self.metrics.try_write() {
                    metrics.record_operation(shard_id, "DEL", hit, latency_ms);
                }

                response
            }
            Command::Expire { key, ttl } => {
                let shard = self.get_shard_for_key_simd(key);
                let shard_id = shard.id;

                let response = shard.expire_optimized(key, *ttl).await;

                // Record metrics
                let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                let hit = matches!(response, Response::Ok);
                if let Ok(mut metrics) = self.metrics.try_write() {
                    metrics.record_operation(shard_id, "EXPIRE", hit, latency_ms);
                }

                response
            }
            Command::Stats => self.get_detailed_stats().await,
            Command::Metrics => self.get_performance_metrics().await,
            Command::Ping => {
                let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                if let Ok(mut metrics) = self.metrics.try_write() {
                    metrics.record_operation(0, "PING", true, latency_ms);
                }
                Response::Pong
            }
        };

        response
    }

    /// Get detailed statistics with new metrics system
    async fn get_detailed_stats(&self) -> Response {
        if let Ok(metrics) = self.metrics.try_read() {
            let stats = metrics.get_stats();
            let json = serde_json::to_string_pretty(&stats)
                .unwrap_or_else(|_| "Error serializing stats".to_string());
            Response::Stats(json)
        } else {
            Response::Error("Failed to read metrics".to_string())
        }
    }

    /// Get detailed performance metrics
    async fn get_performance_metrics(&self) -> Response {
        if let Ok(metrics) = self.metrics.try_read() {
            let stats = metrics.get_stats();
            let performance_summary = format!(
                "ops_per_sec={:.1} hit_ratio={:.2} p99_latency={:.3}ms active_connections={}",
                stats.global.operations_per_second,
                stats.global.cache_hit_ratio,
                stats.latency.p99_ms,
                stats.global.active_connections
            );
            Response::Stats(performance_summary)
        } else {
            Response::Error("Failed to read performance metrics".to_string())
        }
    }

    /// Bulk operations for extreme performance
    pub async fn bulk_put(&self, entries: Vec<(Bytes, Bytes)>) -> usize {
        let mut total_inserted = 0;

        // Group entries by shard
        let mut shard_groups: std::collections::HashMap<usize, Vec<(Bytes, Bytes)>> =
            std::collections::HashMap::new();

        for (key, value) in entries {
            let shard_idx = if self.simd_enabled && key.len() >= 16 {
                (SIMDParser::hash_key_simd(&key) as usize) % self.shards.len()
            } else {
                (crate::utils::hash::hash_key(&key) as usize) % self.shards.len()
            };

            shard_groups
                .entry(shard_idx)
                .or_insert_with(Vec::new)
                .push((key, value));
        }

        // Execute bulk operations per shard
        for (shard_idx, shard_entries) in shard_groups {
            let shard = &self.shards[shard_idx];
            total_inserted += shard.bulk_put_optimized(shard_entries).await;
        }

        total_inserted
    }

    /// Get access to metrics for external use (HTTP endpoint, etc.)
    pub fn get_shared_metrics(&self) -> SharedMetrics {
        Arc::clone(&self.metrics)
    }

    /// Enable/disable optimizations
    pub fn set_zero_copy_enabled(&mut self, enabled: bool) {
        self.zero_copy_enabled = enabled;
        info!(
            "Zero-copy operations: {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    pub fn set_simd_enabled(&mut self, enabled: bool) {
        self.simd_enabled = enabled;
        info!(
            "SIMD operations: {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    pub fn set_lockfree_enabled(&mut self, enabled: bool) {
        self.lockfree_enabled = enabled;
        info!(
            "Lock-free operations: {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }
}

/// Optimized shard with extreme performance features
pub struct OptimizedShard {
    id: usize,
    regular_shard: Arc<Shard>,
    zero_copy_store: ZeroCopyStore,
    lockfree_map: CrabCacheLockFreeMap,
    stats: Arc<Mutex<OptimizedShardStats>>,
    metrics: SharedMetrics,
}

impl OptimizedShard {
    /// Create new optimized shard
    pub fn new(id: usize, max_memory: usize, metrics: SharedMetrics) -> Self {
        Self {
            id,
            regular_shard: Arc::new(Shard::new(id, max_memory)),
            zero_copy_store: ZeroCopyStore::new(max_memory),
            lockfree_map: CrabCacheLockFreeMap::new_optimized(max_memory / 64), // Estimate entries
            stats: Arc::new(Mutex::new(OptimizedShardStats::new(id))),
            metrics,
        }
    }

    /// Zero-copy PUT operation with lock-free caching
    pub async fn put_zero_copy(&self, key: Bytes, value: Bytes, ttl: Option<u64>) -> Response {
        // Store in lock-free map first (fastest write path)
        self.lockfree_map.insert(key.clone(), value.clone());
        self.stats.lock().unwrap().increment_lockfree_ops();

        // Try zero-copy store for persistence
        match self
            .zero_copy_store
            .put_zero_copy(key.clone(), value.clone())
        {
            Ok(()) => {
                self.stats.lock().unwrap().increment_zero_copy_ops();

                // Handle TTL if specified
                if let Some(ttl_seconds) = ttl {
                    // Delegate TTL handling to regular shard for now
                    let cmd = Command::Expire {
                        key: key.clone(),
                        ttl: ttl_seconds,
                    };
                    self.regular_shard.process_command(cmd).await;
                }

                Response::Ok
            }
            Err(e) => {
                warn!("Zero-copy PUT failed: {}, using regular shard", e);
                let cmd = Command::Put { key, value, ttl };
                self.regular_shard.process_command(cmd).await
            }
        }
    }

    /// Zero-copy GET operation with lock-free optimization
    pub async fn get_zero_copy(&self, key: &Bytes) -> Response {
        // Try lock-free map first (fastest path)
        if let Some(value) = self.lockfree_map.get(key) {
            self.stats.lock().unwrap().increment_lockfree_ops();
            return Response::Value(value);
        }

        // Try zero-copy store
        if let Some(value) = self.zero_copy_store.get_zero_copy(key) {
            self.stats.lock().unwrap().increment_zero_copy_ops();
            // Cache in lock-free map for future access
            self.lockfree_map.insert(key.clone(), value.clone());
            return Response::Value(value);
        }

        // Fallback to regular shard
        let cmd = Command::Get { key: key.clone() };
        let response = self.regular_shard.process_command(cmd).await;

        // Cache successful results in lock-free map
        if let Response::Value(ref value) = response {
            self.lockfree_map.insert(key.clone(), value.clone());
            self.stats.lock().unwrap().increment_lockfree_ops();
        }

        response
    }

    /// Zero-copy DELETE operation
    pub async fn del_zero_copy(&self, key: &Bytes) -> Response {
        let mut found = false;

        // Remove from lock-free map
        if self.lockfree_map.remove(key).is_some() {
            found = true;
            self.stats.lock().unwrap().increment_lockfree_ops();
        }

        // Remove from zero-copy store
        if self.zero_copy_store.del_zero_copy(key) {
            found = true;
            self.stats.lock().unwrap().increment_zero_copy_ops();
        }

        // Also remove from regular shard
        let cmd = Command::Del { key: key.clone() };
        let regular_result = self.regular_shard.process_command(cmd).await;

        if found || matches!(regular_result, Response::Ok) {
            Response::Ok
        } else {
            Response::Null
        }
    }

    /// Optimized EXPIRE operation
    pub async fn expire_optimized(&self, key: &Bytes, ttl: u64) -> Response {
        // For now, delegate to regular shard
        // TODO: Implement TTL in lock-free and zero-copy stores
        let cmd = Command::Expire {
            key: key.clone(),
            ttl,
        };
        self.regular_shard.process_command(cmd).await
    }

    /// Bulk PUT operation
    pub async fn bulk_put_optimized(&self, entries: Vec<(Bytes, Bytes)>) -> usize {
        let inserted = self.lockfree_map.bulk_insert(entries);
        self.stats.lock().unwrap().add_lockfree_ops(inserted as u64);
        inserted
    }

    /// Process regular command (fallback)
    pub async fn process_command(&self, command: Command) -> Response {
        self.regular_shard.process_command(command).await
    }

    /// Get optimized statistics
    pub async fn get_optimized_stats(&self) -> OptimizedShardStats {
        let (_, key_count, memory_used, _) = self.regular_shard.get_stats().await;
        let lockfree_stats = self.lockfree_map.stats();
        let zero_copy_metrics = self.zero_copy_store.metrics();

        let stats = self.stats.lock().unwrap();
        OptimizedShardStats {
            shard_id: self.id,
            key_count,
            memory_used,
            zero_copy_operations: stats.zero_copy_operations,
            simd_operations: stats.simd_operations,
            lockfree_operations: stats.lockfree_operations,
            contention_rate: lockfree_stats.contention_rate,
            zero_copy_efficiency: zero_copy_metrics.zero_copy_efficiency(),
        }
    }

    /// Start optimized cleanup task
    pub fn start_optimized_cleanup(shard: Arc<OptimizedShard>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;

                // Cleanup zero-copy store
                if let Err(e) = shard.zero_copy_store.compact_arena() {
                    warn!("Arena compaction failed: {}", e);
                }

                // Regular shard cleanup is handled by its own task
            }
        });
    }
}

/// Optimized shard statistics
#[derive(Debug, Clone)]
pub struct OptimizedShardStats {
    pub shard_id: usize,
    pub key_count: usize,
    pub memory_used: usize,
    pub zero_copy_operations: u64,
    pub simd_operations: u64,
    pub lockfree_operations: u64,
    pub contention_rate: f64,
    pub zero_copy_efficiency: f64,
}

impl OptimizedShardStats {
    fn new(shard_id: usize) -> Self {
        Self {
            shard_id,
            key_count: 0,
            memory_used: 0,
            zero_copy_operations: 0,
            simd_operations: 0,
            lockfree_operations: 0,
            contention_rate: 0.0,
            zero_copy_efficiency: 0.0,
        }
    }

    fn increment_zero_copy_ops(&mut self) {
        self.zero_copy_operations += 1;
    }

    fn increment_simd_ops(&mut self) {
        self.simd_operations += 1;
    }

    fn increment_lockfree_ops(&mut self) {
        self.lockfree_operations += 1;
    }

    fn add_lockfree_ops(&mut self, count: u64) {
        self.lockfree_operations += count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_optimized_shard_manager() {
        let manager = OptimizedShardManager::new(4, 1024 * 1024);

        // Test optimized PUT
        let put_cmd = Command::Put {
            key: Bytes::from("test_key"),
            value: Bytes::from("test_value"),
            ttl: None,
        };

        let response = manager.process_command_optimized(put_cmd).await;
        assert!(matches!(response, Response::Ok));

        // Test optimized GET
        let get_cmd = Command::Get {
            key: Bytes::from("test_key"),
        };

        let response = manager.process_command_optimized(get_cmd).await;
        if let Response::Value(value) = response {
            assert_eq!(value, Bytes::from("test_value"));
        } else {
            panic!("Expected value response");
        }
    }

    #[tokio::test]
    async fn test_bulk_operations() {
        let manager = OptimizedShardManager::new(2, 1024 * 1024);

        let entries = vec![
            (Bytes::from("key1"), Bytes::from("value1")),
            (Bytes::from("key2"), Bytes::from("value2")),
            (Bytes::from("key3"), Bytes::from("value3")),
        ];

        let inserted = manager.bulk_put(entries).await;
        assert_eq!(inserted, 3);
    }

    #[tokio::test]
    async fn test_optimization_toggles() {
        let mut manager = OptimizedShardManager::new(2, 1024 * 1024);

        // Test disabling optimizations
        manager.set_zero_copy_enabled(false);
        manager.set_simd_enabled(false);
        manager.set_lockfree_enabled(false);

        // Should still work with regular operations
        let put_cmd = Command::Put {
            key: Bytes::from("test_key"),
            value: Bytes::from("test_value"),
            ttl: None,
        };

        let response = manager.process_command_optimized(put_cmd).await;
        assert!(matches!(response, Response::Ok));
    }
}
