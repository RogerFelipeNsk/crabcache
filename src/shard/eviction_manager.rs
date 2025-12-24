//! Shard manager with integrated TinyLFU eviction system

use super::Shard;
use crate::eviction::{
    EvictionConfig, EvictionPolicy, MemoryPressureCoordinator, MemoryPressureMonitor, TinyLFU,
};
use crate::metrics::SharedMetrics;
use crate::protocol::commands::{Command, Response};
use bytes::Bytes;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

/// Shard manager with integrated eviction system
pub struct EvictionShardManager {
    /// Optimized shards with eviction
    shards: Vec<Arc<EvictionShard>>,
    /// Memory pressure coordinator
    memory_coordinator: Arc<RwLock<MemoryPressureCoordinator>>,
    /// Global metrics
    metrics: SharedMetrics,
    /// Eviction configuration
    eviction_config: EvictionConfig,
}

impl EvictionShardManager {
    /// Create new eviction-enabled shard manager
    pub fn new(
        num_shards: usize,
        max_memory_per_shard: usize,
        eviction_config: EvictionConfig,
    ) -> Result<Self, String> {
        eviction_config.validate()?;

        info!(
            "Creating EvictionShardManager with {} shards, {}B per shard, TinyLFU enabled",
            num_shards, max_memory_per_shard
        );

        let metrics = crate::metrics::create_shared_metrics(num_shards);
        let mut memory_coordinator = MemoryPressureCoordinator::new();

        let shards: Result<Vec<Arc<EvictionShard>>, String> = (0..num_shards)
            .map(|id| {
                let shard = EvictionShard::new(
                    id,
                    max_memory_per_shard,
                    eviction_config.clone(),
                    Arc::clone(&metrics),
                )?;

                // Add memory monitor to coordinator
                memory_coordinator.add_monitor(Arc::clone(&shard.memory_monitor));

                Ok(Arc::new(shard))
            })
            .collect();

        let shards = shards?;

        // Start background eviction task
        let coordinator_clone = Arc::new(RwLock::new(memory_coordinator));
        let shards_clone = shards.clone();
        let coordinator_for_task = Arc::clone(&coordinator_clone);

        tokio::spawn(async move {
            Self::background_eviction_task(coordinator_for_task, shards_clone).await;
        });

        Ok(Self {
            shards,
            memory_coordinator: coordinator_clone,
            metrics,
            eviction_config,
        })
    }

    /// Get shard for key
    pub fn get_shard_for_key(&self, key: &[u8]) -> &Arc<EvictionShard> {
        let hash = crate::utils::hash::hash_key(key) as usize;
        let shard_index = hash % self.shards.len();
        &self.shards[shard_index]
    }

    /// Process command with eviction support
    pub async fn process_command(&self, command: Command) -> Response {
        let start_time = Instant::now();

        let response = match &command {
            Command::Put { key, value, ttl } => {
                let shard = self.get_shard_for_key(key);
                let response = shard
                    .put_with_eviction(
                        String::from_utf8_lossy(key).to_string(),
                        value.to_vec(),
                        *ttl,
                    )
                    .await;

                // Record metrics
                let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                let hit = matches!(response, Response::Ok);
                if let Ok(mut metrics) = self.metrics.try_write() {
                    metrics.record_operation(shard.id, "PUT", hit, latency_ms);
                }

                response
            }
            Command::Get { key } => {
                let shard = self.get_shard_for_key(key);
                let response = shard.get_with_eviction(&String::from_utf8_lossy(key)).await;

                // Record metrics
                let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                let hit = matches!(response, Response::Value(_));
                if let Ok(mut metrics) = self.metrics.try_write() {
                    metrics.record_operation(shard.id, "GET", hit, latency_ms);
                }

                response
            }
            Command::Del { key } => {
                let shard = self.get_shard_for_key(key);
                let response = shard
                    .delete_with_eviction(&String::from_utf8_lossy(key))
                    .await;

                // Record metrics
                let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                let hit = matches!(response, Response::Ok);
                if let Ok(mut metrics) = self.metrics.try_write() {
                    metrics.record_operation(shard.id, "DEL", hit, latency_ms);
                }

                response
            }
            Command::Stats => self.get_eviction_stats().await,
            Command::Ping => Response::Pong,
            _ => {
                // Fallback to regular shard for other commands
                let shard = self.get_shard_for_key(b"default");
                shard.fallback_shard.process_command(command).await
            }
        };

        response
    }

    /// Get detailed eviction statistics
    async fn get_eviction_stats(&self) -> Response {
        let mut stats = serde_json::Map::new();

        // Global eviction stats
        let coordinator = self.memory_coordinator.read().unwrap();
        let memory_stats = coordinator.memory_stats();

        stats.insert(
            "total_memory_usage".to_string(),
            serde_json::Value::Number(coordinator.total_usage().into()),
        );
        stats.insert(
            "total_memory_limit".to_string(),
            serde_json::Value::Number(coordinator.total_limit().into()),
        );
        stats.insert(
            "overall_usage_ratio".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(coordinator.overall_usage_ratio()).unwrap(),
            ),
        );

        // Per-shard eviction stats
        let mut shard_stats = Vec::new();
        for (i, shard) in self.shards.iter().enumerate() {
            let eviction_metrics = shard.eviction_cache.read().unwrap().metrics().snapshot();
            let memory_stat = &memory_stats[i];

            let mut shard_stat = serde_json::Map::new();
            shard_stat.insert("shard_id".to_string(), serde_json::Value::Number(i.into()));
            shard_stat.insert("cache_hits".to_string(), eviction_metrics.cache_hits.into());
            shard_stat.insert(
                "cache_misses".to_string(),
                eviction_metrics.cache_misses.into(),
            );
            shard_stat.insert(
                "hit_ratio".to_string(),
                serde_json::Number::from_f64(eviction_metrics.hit_ratio)
                    .unwrap()
                    .into(),
            );
            shard_stat.insert("evictions".to_string(), eviction_metrics.evictions.into());
            shard_stat.insert(
                "admissions_accepted".to_string(),
                eviction_metrics.admissions_accepted.into(),
            );
            shard_stat.insert(
                "admissions_rejected".to_string(),
                eviction_metrics.admissions_rejected.into(),
            );
            shard_stat.insert(
                "admission_ratio".to_string(),
                serde_json::Number::from_f64(eviction_metrics.admission_ratio)
                    .unwrap()
                    .into(),
            );
            shard_stat.insert("memory_usage".to_string(), memory_stat.current_usage.into());
            shard_stat.insert("memory_limit".to_string(), memory_stat.memory_limit.into());
            shard_stat.insert(
                "memory_pressure".to_string(),
                serde_json::Number::from_f64(memory_stat.pressure_level)
                    .unwrap()
                    .into(),
            );

            // Add items count from TinyLFU cache
            let items_count = shard.eviction_cache.read().unwrap().len();
            shard_stat.insert("items".to_string(), items_count.into());

            shard_stats.push(serde_json::Value::Object(shard_stat));
        }

        stats.insert("shards".to_string(), serde_json::Value::Array(shard_stats));

        let json = serde_json::to_string_pretty(&stats)
            .unwrap_or_else(|_| "Error serializing eviction stats".to_string());

        Response::Stats(json)
    }

    /// Background task for coordinated eviction
    async fn background_eviction_task(
        coordinator: Arc<RwLock<MemoryPressureCoordinator>>,
        shards: Vec<Arc<EvictionShard>>,
    ) {
        let mut interval = interval(Duration::from_millis(100)); // Check every 100ms

        loop {
            interval.tick().await;

            // Check if any shard needs eviction
            let shards_needing_eviction = {
                let coord = coordinator.read().unwrap();
                coord.shards_needing_eviction()
            };

            if !shards_needing_eviction.is_empty() {
                debug!("Shards needing eviction: {:?}", shards_needing_eviction);

                for shard_id in shards_needing_eviction {
                    if let Some(shard) = shards.get(shard_id) {
                        // Calculate how many items to evict
                        let bytes_to_free = shard.memory_monitor.bytes_to_free();
                        let avg_item_size = 1024; // Estimate 1KB per item
                        let items_to_evict = (bytes_to_free / avg_item_size).max(1);

                        // Perform eviction
                        let evicted: Vec<(String, Vec<u8>)> =
                            shard.force_eviction(items_to_evict).await;

                        if !evicted.is_empty() {
                            info!("Evicted {} items from shard {}", evicted.len(), shard_id);

                            // Update memory usage
                            let freed_bytes: usize = evicted
                                .iter()
                                .map(|(k, v): &(String, Vec<u8>)| k.len() + v.len())
                                .sum();
                            shard.memory_monitor.update_usage(-(freed_bytes as isize));
                        }
                    }
                }
            }
        }
    }

    /// Get shard ID for key (using same hash function as get_shard_for_key)
    pub fn get_shard_id(&self, key: &str) -> usize {
        let hash = crate::utils::hash::hash_key(key.as_bytes()) as usize;
        hash % self.shards.len()
    }

    /// Get number of shards
    pub fn num_shards(&self) -> usize {
        self.shards.len()
    }

    /// Get comprehensive metrics
    pub async fn get_metrics(&self) -> serde_json::Value {
        let mut stats = serde_json::Map::new();

        // Global eviction stats
        let coordinator = self.memory_coordinator.read().unwrap();

        stats.insert(
            "total_memory_usage".to_string(),
            serde_json::Value::Number(coordinator.total_usage().into()),
        );
        stats.insert(
            "total_memory_limit".to_string(),
            serde_json::Value::Number(coordinator.total_limit().into()),
        );
        stats.insert(
            "overall_usage_ratio".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(coordinator.overall_usage_ratio()).unwrap(),
            ),
        );

        // Per-shard eviction stats
        let mut shard_stats = Vec::new();
        for (i, shard) in self.shards.iter().enumerate() {
            let eviction_metrics = shard.eviction_cache.read().unwrap().metrics().snapshot();

            let mut shard_stat = serde_json::Map::new();
            shard_stat.insert("shard_id".to_string(), serde_json::Value::Number(i.into()));
            shard_stat.insert("cache_hits".to_string(), eviction_metrics.cache_hits.into());
            shard_stat.insert(
                "cache_misses".to_string(),
                eviction_metrics.cache_misses.into(),
            );
            shard_stat.insert(
                "hit_ratio".to_string(),
                serde_json::Number::from_f64(eviction_metrics.hit_ratio)
                    .unwrap()
                    .into(),
            );
            shard_stat.insert("evictions".to_string(), eviction_metrics.evictions.into());
            shard_stat.insert(
                "admissions_accepted".to_string(),
                eviction_metrics.admissions_accepted.into(),
            );
            shard_stat.insert(
                "admissions_rejected".to_string(),
                eviction_metrics.admissions_rejected.into(),
            );
            shard_stat.insert(
                "admission_ratio".to_string(),
                serde_json::Number::from_f64(eviction_metrics.admission_ratio)
                    .unwrap()
                    .into(),
            );
            shard_stat.insert(
                "memory_usage".to_string(),
                shard.memory_monitor.current_usage().into(),
            );
            shard_stat.insert(
                "memory_limit".to_string(),
                shard.memory_monitor.memory_limit().into(),
            );
            shard_stat.insert(
                "memory_pressure".to_string(),
                serde_json::Number::from_f64(shard.memory_monitor.pressure_level())
                    .unwrap()
                    .into(),
            );

            shard_stats.push(serde_json::Value::Object(shard_stat));
        }

        stats.insert(
            "eviction".to_string(),
            serde_json::json!({
                "total_memory_usage": coordinator.total_usage(),
                "total_memory_limit": coordinator.total_limit(),
                "overall_usage_ratio": coordinator.overall_usage_ratio(),
                "shards": shard_stats
            }),
        );

        serde_json::Value::Object(stats)
    }

    /// Shutdown gracefully
    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Shutting down eviction shard manager");

        // Stop background eviction task (it will stop when coordinator is dropped)
        // The background task will terminate when the coordinator Arc is dropped

        info!("Eviction shard manager shutdown complete");
        Ok(())
    }
}

/// Individual shard with eviction capabilities
pub struct EvictionShard {
    /// Shard ID
    pub id: usize,
    /// TinyLFU eviction cache
    pub eviction_cache: Arc<RwLock<TinyLFU>>,
    /// Memory pressure monitor
    pub memory_monitor: Arc<MemoryPressureMonitor>,
    /// Fallback to regular shard for non-cache operations
    pub fallback_shard: Arc<Shard>,
    /// Metrics
    metrics: SharedMetrics,
}

impl EvictionShard {
    /// Create new eviction shard
    pub fn new(
        id: usize,
        max_memory: usize,
        eviction_config: EvictionConfig,
        metrics: SharedMetrics,
    ) -> Result<Self, String> {
        let tinylfu = TinyLFU::new(eviction_config.clone())?;
        let memory_monitor = Arc::new(MemoryPressureMonitor::new(
            id,
            max_memory,
            eviction_config.memory_high_watermark,
            eviction_config.memory_low_watermark,
        )?);

        let fallback_shard = Arc::new(Shard::new(id, max_memory));

        Ok(Self {
            id,
            eviction_cache: Arc::new(RwLock::new(tinylfu)),
            memory_monitor,
            fallback_shard,
            metrics,
        })
    }

    /// PUT with eviction support and error recovery
    pub async fn put_with_eviction(
        &self,
        key: String,
        value: Vec<u8>,
        ttl: Option<u64>,
    ) -> Response {
        let item_size = key.len() + value.len();

        // Check if we need to evict before inserting
        if self.memory_monitor.should_evict() {
            let items_to_evict = 1; // Evict at least one item
            self.force_eviction(items_to_evict).await;
        }

        // Try TinyLFU cache first
        match self.try_tinylfu_put(&key, &value) {
            Ok(evicted) => {
                // Update memory usage
                self.memory_monitor.update_usage(item_size as isize);

                // If something was evicted, update memory usage
                if let Some((evicted_key, evicted_value)) = evicted {
                    let evicted_size = evicted_key.len() + evicted_value.len();
                    self.memory_monitor.update_usage(-(evicted_size as isize));
                }

                Response::Ok
            }
            Err(e) => {
                warn!(
                    "TinyLFU PUT failed for key '{}': {}, falling back to regular shard",
                    key, e
                );

                // Fallback to regular shard
                let cmd = Command::Put {
                    key: Bytes::from(key),
                    value: Bytes::from(value),
                    ttl,
                };
                self.fallback_shard.process_command(cmd).await
            }
        }
    }

    /// GET with eviction support and error recovery
    pub async fn get_with_eviction(&self, key: &str) -> Response {
        // Try TinyLFU cache first
        match self.try_tinylfu_get(key) {
            Ok(Some(value)) => Response::Value(Bytes::from(value)),
            Ok(None) => {
                // Try fallback shard
                let cmd = Command::Get {
                    key: Bytes::from(key.to_string()),
                };
                match self.fallback_shard.process_command(cmd).await {
                    Response::Value(value) => {
                        // Found in fallback, try to add to TinyLFU for future hits
                        if let Err(e) = self.try_tinylfu_put(key, &value) {
                            debug!("Failed to add fallback item to TinyLFU: {}", e);
                        }
                        Response::Value(value)
                    }
                    other => other,
                }
            }
            Err(e) => {
                warn!(
                    "TinyLFU GET failed for key '{}': {}, falling back to regular shard",
                    key, e
                );

                // Fallback to regular shard
                let cmd = Command::Get {
                    key: Bytes::from(key.to_string()),
                };
                self.fallback_shard.process_command(cmd).await
            }
        }
    }

    /// DELETE with eviction support and error recovery
    pub async fn delete_with_eviction(&self, key: &str) -> Response {
        let mut found_in_tinylfu = false;

        // Try TinyLFU cache first
        match self.try_tinylfu_remove(key) {
            Ok(Some(value)) => {
                // Update memory usage
                let freed_size = key.len() + value.len();
                self.memory_monitor.update_usage(-(freed_size as isize));
                found_in_tinylfu = true;
            }
            Ok(None) => {
                // Not found in TinyLFU
            }
            Err(e) => {
                warn!(
                    "TinyLFU DELETE failed for key '{}': {}, continuing with fallback",
                    key, e
                );
            }
        }

        // Also try fallback shard
        let cmd = Command::Del {
            key: Bytes::from(key.to_string()),
        };
        let fallback_response = self.fallback_shard.process_command(cmd).await;

        // Return Ok if found in either cache
        if found_in_tinylfu || matches!(fallback_response, Response::Ok) {
            Response::Ok
        } else {
            Response::Null
        }
    }

    /// Try TinyLFU PUT operation with error handling
    fn try_tinylfu_put(
        &self,
        key: &str,
        value: &[u8],
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        match self.eviction_cache.try_write() {
            Ok(mut cache) => Ok(cache.put(key.to_string(), value.to_vec())),
            Err(_) => Err("Failed to acquire write lock on TinyLFU cache".to_string()),
        }
    }

    /// Try TinyLFU GET operation with error handling
    fn try_tinylfu_get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        match self.eviction_cache.try_write() {
            Ok(mut cache) => Ok(cache.get(key)),
            Err(_) => Err("Failed to acquire write lock on TinyLFU cache".to_string()),
        }
    }

    /// Try TinyLFU REMOVE operation with error handling
    fn try_tinylfu_remove(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        match self.eviction_cache.try_write() {
            Ok(mut cache) => Ok(cache.remove(key)),
            Err(_) => Err("Failed to acquire write lock on TinyLFU cache".to_string()),
        }
    }

    /// Force eviction of items with error handling
    pub async fn force_eviction(&self, count: usize) -> Vec<(String, Vec<u8>)> {
        match self.eviction_cache.try_write() {
            Ok(mut cache) => cache.evict_items(count),
            Err(e) => {
                error!(
                    "Failed to acquire lock for force eviction on shard {}: {}",
                    self.id, e
                );

                // If TinyLFU eviction fails, try to free memory using fallback shard
                // This is a last resort to prevent memory exhaustion
                warn!("Attempting emergency memory cleanup on shard {}", self.id);

                // Reset memory monitor to prevent infinite eviction attempts
                let current_usage = self.memory_monitor.current_usage();
                let target_usage = (current_usage as f64 * 0.8) as usize; // Reduce by 20%
                self.memory_monitor.set_usage(target_usage);

                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eviction_shard_creation() {
        let config = EvictionConfig::default();
        let metrics = crate::metrics::create_shared_metrics(1);

        let shard = EvictionShard::new(0, 1000, config, metrics);
        assert!(shard.is_ok());

        let shard = shard.unwrap();
        assert_eq!(shard.id, 0);
        assert_eq!(shard.memory_monitor.memory_limit(), 1000);
    }

    #[tokio::test]
    async fn test_put_get_with_eviction() {
        let config = EvictionConfig {
            max_capacity: 2, // Small capacity for testing
            min_items_threshold: 1, // Must be less than max_capacity
            ..Default::default()
        };
        let metrics = crate::metrics::create_shared_metrics(1);

        let shard = EvictionShard::new(0, 1000, config, metrics).unwrap();

        // Put some items
        let response = shard
            .put_with_eviction("key1".to_string(), b"value1".to_vec(), None)
            .await;
        assert!(matches!(response, Response::Ok));

        let response = shard
            .put_with_eviction("key2".to_string(), b"value2".to_vec(), None)
            .await;
        assert!(matches!(response, Response::Ok));

        // Get items
        let response = shard.get_with_eviction("key1").await;
        assert!(matches!(response, Response::Value(_)));

        let response = shard.get_with_eviction("key2").await;
        assert!(matches!(response, Response::Value(_)));

        // Add third item, should trigger eviction
        let response = shard
            .put_with_eviction("key3".to_string(), b"value3".to_vec(), None)
            .await;
        assert!(matches!(response, Response::Ok));
    }

    #[tokio::test]
    async fn test_delete_with_eviction() {
        let config = EvictionConfig::default();
        let metrics = crate::metrics::create_shared_metrics(1);

        let shard = EvictionShard::new(0, 1000, config, metrics).unwrap();

        // Put and delete
        shard
            .put_with_eviction("key1".to_string(), b"value1".to_vec(), None)
            .await;

        let response = shard.delete_with_eviction("key1").await;
        assert!(matches!(response, Response::Ok));

        let response = shard.get_with_eviction("key1").await;
        assert!(matches!(response, Response::Null));
    }

    #[tokio::test]
    async fn test_force_eviction() {
        let config = EvictionConfig {
            max_capacity: 5,
            min_items_threshold: 2, // Must be less than max_capacity
            ..Default::default()
        };
        let metrics = crate::metrics::create_shared_metrics(1);

        let shard = EvictionShard::new(0, 1000, config, metrics).unwrap();

        // Fill cache
        for i in 0..5 {
            shard
                .put_with_eviction(
                    format!("key{}", i),
                    format!("value{}", i).into_bytes(),
                    None,
                )
                .await;
        }

        // Force eviction
        let evicted = shard.force_eviction(2).await;
        assert_eq!(evicted.len(), 2);
    }
}
