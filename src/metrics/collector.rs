use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use super::histogram::LatencyHistogram;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalMetrics {
    pub uptime_seconds: u64,
    pub total_operations: u64,
    pub operations_per_second: f64,
    pub memory_used_bytes: u64,
    pub cache_hit_ratio: f64,
    pub total_connections: u64,
    pub active_connections: u64,
}

#[derive(Debug)]
pub struct ShardMetrics {
    pub operations_count: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub evictions: AtomicU64,
    pub memory_used: AtomicU64,
    pub items_count: AtomicU64,
    pub get_operations: AtomicU64,
    pub put_operations: AtomicU64,
    pub del_operations: AtomicU64,
    pub expire_operations: AtomicU64,
}

impl ShardMetrics {
    pub fn new() -> Self {
        Self {
            operations_count: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            memory_used: AtomicU64::new(0),
            items_count: AtomicU64::new(0),
            get_operations: AtomicU64::new(0),
            put_operations: AtomicU64::new(0),
            del_operations: AtomicU64::new(0),
            expire_operations: AtomicU64::new(0),
        }
    }

    pub fn to_serializable(&self) -> SerializableShardMetrics {
        SerializableShardMetrics {
            operations: self.operations_count.load(Ordering::Relaxed),
            hits: self.cache_hits.load(Ordering::Relaxed),
            misses: self.cache_misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            memory_bytes: self.memory_used.load(Ordering::Relaxed),
            items: self.items_count.load(Ordering::Relaxed),
            get_ops: self.get_operations.load(Ordering::Relaxed),
            put_ops: self.put_operations.load(Ordering::Relaxed),
            del_ops: self.del_operations.load(Ordering::Relaxed),
            expire_ops: self.expire_operations.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableShardMetrics {
    pub operations: u64,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub memory_bytes: u64,
    pub items: u64,
    pub get_ops: u64,
    pub put_ops: u64,
    pub del_ops: u64,
    pub expire_ops: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyMetrics {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub p99_9_ms: f64,
    pub mean_ms: f64,
    pub max_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub global: GlobalMetrics,
    pub shards: Vec<SerializableShardMetrics>,
    pub latency: LatencyMetrics,
}

pub struct MetricsCollector {
    pub shard_metrics: Vec<ShardMetrics>,
    pub start_time: Instant,
    pub total_connections: AtomicU64,
    pub active_connections: AtomicU64,
    pub latency_histograms: HashMap<String, LatencyHistogram>,
}

impl MetricsCollector {
    pub fn new(num_shards: usize) -> Self {
        let mut latency_histograms = HashMap::new();
        latency_histograms.insert("get".to_string(), LatencyHistogram::new());
        latency_histograms.insert("put".to_string(), LatencyHistogram::new());
        latency_histograms.insert("del".to_string(), LatencyHistogram::new());
        latency_histograms.insert("expire".to_string(), LatencyHistogram::new());
        latency_histograms.insert("ping".to_string(), LatencyHistogram::new());

        Self {
            shard_metrics: (0..num_shards).map(|_| ShardMetrics::new()).collect(),
            start_time: Instant::now(),
            total_connections: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            latency_histograms,
        }
    }

    pub fn record_operation(&mut self, shard_id: usize, operation: &str, hit: bool, latency_ms: f64) {
        if shard_id < self.shard_metrics.len() {
            let shard = &self.shard_metrics[shard_id];
            shard.operations_count.fetch_add(1, Ordering::Relaxed);

            if hit {
                shard.cache_hits.fetch_add(1, Ordering::Relaxed);
            } else {
                shard.cache_misses.fetch_add(1, Ordering::Relaxed);
            }

            match operation {
                "GET" => { shard.get_operations.fetch_add(1, Ordering::Relaxed); },
                "PUT" => { shard.put_operations.fetch_add(1, Ordering::Relaxed); },
                "DEL" => { shard.del_operations.fetch_add(1, Ordering::Relaxed); },
                "EXPIRE" => { shard.expire_operations.fetch_add(1, Ordering::Relaxed); },
                _ => {}
            }

            // Record latency
            if let Some(histogram) = self.latency_histograms.get_mut(&operation.to_lowercase()) {
                histogram.record(latency_ms);
            }
        }
    }

    pub fn record_eviction(&self, shard_id: usize, count: u64) {
        if shard_id < self.shard_metrics.len() {
            self.shard_metrics[shard_id].evictions.fetch_add(count, Ordering::Relaxed);
        }
    }

    pub fn update_shard_memory(&self, shard_id: usize, memory_bytes: u64) {
        if shard_id < self.shard_metrics.len() {
            self.shard_metrics[shard_id].memory_used.store(memory_bytes, Ordering::Relaxed);
        }
    }

    pub fn update_shard_items(&self, shard_id: usize, items_count: u64) {
        if shard_id < self.shard_metrics.len() {
            self.shard_metrics[shard_id].items_count.store(items_count, Ordering::Relaxed);
        }
    }

    pub fn increment_connections(&self) {
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> StatsResponse {
        let uptime = self.start_time.elapsed().as_secs();
        
        // Calculate totals
        let mut total_operations = 0u64;
        let mut total_hits = 0u64;
        let mut total_misses = 0u64;
        let mut total_memory = 0u64;
        let mut total_items = 0u64;

        let serializable_shards: Vec<SerializableShardMetrics> = self.shard_metrics
            .iter()
            .map(|shard| {
                let metrics = shard.to_serializable();
                total_operations += metrics.operations;
                total_hits += metrics.hits;
                total_misses += metrics.misses;
                total_memory += metrics.memory_bytes;
                total_items += metrics.items;
                metrics
            })
            .collect();

        let operations_per_second = if uptime > 0 {
            total_operations as f64 / uptime as f64
        } else {
            0.0
        };

        let cache_hit_ratio = if total_operations > 0 {
            total_hits as f64 / total_operations as f64
        } else {
            0.0
        };

        // Calculate combined latency metrics
        let latency = self.calculate_combined_latency();

        StatsResponse {
            global: GlobalMetrics {
                uptime_seconds: uptime,
                total_operations,
                operations_per_second,
                memory_used_bytes: total_memory,
                cache_hit_ratio,
                total_connections: self.total_connections.load(Ordering::Relaxed),
                active_connections: self.active_connections.load(Ordering::Relaxed),
            },
            shards: serializable_shards,
            latency,
        }
    }

    fn calculate_combined_latency(&self) -> LatencyMetrics {
        let mut all_samples = Vec::new();
        
        for histogram in self.latency_histograms.values() {
            all_samples.extend(histogram.get_samples());
        }

        if all_samples.is_empty() {
            return LatencyMetrics {
                p50_ms: 0.0,
                p95_ms: 0.0,
                p99_ms: 0.0,
                p99_9_ms: 0.0,
                mean_ms: 0.0,
                max_ms: 0.0,
            };
        }

        all_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let len = all_samples.len();

        LatencyMetrics {
            p50_ms: all_samples[len * 50 / 100],
            p95_ms: all_samples[len * 95 / 100],
            p99_ms: all_samples[len * 99 / 100],
            p99_9_ms: all_samples[len * 999 / 1000],
            mean_ms: all_samples.iter().sum::<f64>() / len as f64,
            max_ms: all_samples[len - 1],
        }
    }

    pub fn get_shard_stats(&self, shard_id: usize) -> Option<SerializableShardMetrics> {
        if shard_id < self.shard_metrics.len() {
            Some(self.shard_metrics[shard_id].to_serializable())
        } else {
            None
        }
    }
}