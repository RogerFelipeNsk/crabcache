//! Shard manager implementation

use super::Shard;
use crate::protocol::commands::{Command, Response};
use crate::utils::hash::hash_key;
use std::sync::Arc;
use tracing::{debug, info};

/// Manages multiple shards
pub struct ShardManager {
    shards: Vec<Arc<Shard>>,
}

impl ShardManager {
    pub fn new(num_shards: usize, max_memory_per_shard: usize) -> Self {
        info!("Creating ShardManager with {} shards, {}B per shard", num_shards, max_memory_per_shard);
        let shards: Vec<Arc<Shard>> = (0..num_shards)
            .map(|id| Arc::new(Shard::new(id, max_memory_per_shard)))
            .collect();
        
        // Start TTL cleanup tasks for all shards
        for shard in &shards {
            Shard::start_ttl_cleanup(Arc::clone(shard));
        }
        
        Self { shards }
    }
    
    /// Get the shard for a given key
    pub fn get_shard_for_key(&self, key: &[u8]) -> &Arc<Shard> {
        let hash = hash_key(key);
        let shard_index = (hash as usize) % self.shards.len();
        debug!("Key {} -> shard {}", String::from_utf8_lossy(key), shard_index);
        &self.shards[shard_index]
    }
    
    /// Process a command by routing it to the appropriate shard
    pub async fn process_command(&self, command: Command) -> Response {
        match &command {
            Command::Put { key, .. } | Command::Get { key } | Command::Del { key } | Command::Expire { key, .. } => {
                let shard = self.get_shard_for_key(key);
                shard.process_command(command).await
            }
            Command::Stats => {
                // Aggregate stats from all shards
                let mut all_stats = Vec::new();
                let mut total_keys = 0;
                let mut total_memory = 0;
                let mut total_max_memory = 0;
                
                for shard in &self.shards {
                    let (shard_id, key_count, memory_used, max_memory) = shard.get_stats().await;
                    all_stats.push(format!("shard_{}: {} keys, {}B/{}B", 
                                          shard_id, key_count, memory_used, max_memory));
                    total_keys += key_count;
                    total_memory += memory_used;
                    total_max_memory += max_memory;
                }
                
                all_stats.push(format!("total: {} keys, {}B/{}B memory", 
                                      total_keys, total_memory, total_max_memory));
                Response::Stats(all_stats.join(", "))
            }
            Command::Metrics => {
                // Return performance metrics (similar to stats but focused on performance)
                let mut metrics = Vec::new();
                let mut total_keys = 0;
                let mut total_memory = 0;
                let mut total_max_memory = 0;
                
                for shard in &self.shards {
                    let (shard_id, key_count, memory_used, max_memory) = shard.get_stats().await;
                    total_keys += key_count;
                    total_memory += memory_used;
                    total_max_memory += max_memory;
                }
                
                let memory_utilization = if total_max_memory > 0 {
                    (total_memory as f64 / total_max_memory as f64) * 100.0
                } else {
                    0.0
                };
                
                metrics.push(format!("keys={}", total_keys));
                metrics.push(format!("memory_used={}", total_memory));
                metrics.push(format!("memory_max={}", total_max_memory));
                metrics.push(format!("memory_utilization={:.1}%", memory_utilization));
                metrics.push(format!("shards={}", self.shards.len()));
                
                Response::Stats(metrics.join(" "))
            }
            Command::Ping => Response::Pong,
        }
    }
    
    /// Get total number of keys across all shards
    pub async fn get_total_keys(&self) -> usize {
        let mut total = 0;
        for shard in &self.shards {
            let (_, key_count, _, _) = shard.get_stats().await;
            total += key_count;
        }
        total
    }
    
    /// Get number of shards
    pub fn num_shards(&self) -> usize {
        self.shards.len()
    }
    
    /// Background cleanup of expired items across all shards
    pub async fn cleanup_expired_all(&self) -> usize {
        let mut total_removed = 0;
        for shard in &self.shards {
            total_removed += shard.cleanup_expired().await;
        }
        total_removed
    }
}
