//! Individual shard implementation

use crate::protocol::commands::{Command, Response};
use crate::store::ShardStore;
use crate::ttl::TTLWheel;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::debug;
use std::sync::Arc;

/// A single cache shard
pub struct Shard {
    id: usize,
    /// Optimized storage with binary layout
    store: RwLock<ShardStore>,
    /// TTL wheel for expiration tracking
    ttl_wheel: RwLock<TTLWheel>,
}

impl Shard {
    pub fn new(id: usize, max_memory: usize) -> Self {
        Self { 
            id,
            store: RwLock::new(ShardStore::new(max_memory)),
            ttl_wheel: RwLock::new(TTLWheel::new(3600, 1)), // 1-hour wheel, 1-second granularity
        }
    }
    
    /// Start background TTL cleanup task
    pub fn start_ttl_cleanup(shard: Arc<Self>) {
        let shard_id = shard.id;
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1)); // Tick every second
            
            loop {
                interval.tick().await;
                
                // Tick the TTL wheel and get expired keys
                let expired_keys = {
                    let mut ttl_wheel = shard.ttl_wheel.write().await;
                    ttl_wheel.tick()
                };
                
                if !expired_keys.is_empty() {
                    debug!("Shard {}: TTL cleanup found {} expired keys", shard_id, expired_keys.len());
                    
                    // Remove expired keys from store
                    let mut store = shard.store.write().await;
                    for key in expired_keys {
                        store.del(&key);
                    }
                }
                
                // Also run periodic cleanup of expired items in store
                let removed = {
                    let mut store = shard.store.write().await;
                    store.cleanup_expired()
                };
                
                if removed > 0 {
                    debug!("Shard {}: Store cleanup removed {} expired items", shard_id, removed);
                }
            }
        });
    }
    
    /// Process a command on this shard
    pub async fn process_command(&self, command: Command) -> Response {
        match command {
            Command::Put { key, value, ttl } => {
                debug!("Shard {}: PUT {} = {} (ttl: {:?})", 
                       self.id, String::from_utf8_lossy(&key), 
                       String::from_utf8_lossy(&value), ttl);
                
                let mut store = self.store.write().await;
                if store.put(key.clone(), value, ttl) {
                    // Add to TTL wheel if TTL is specified
                    if let Some(ttl_seconds) = ttl {
                        let mut ttl_wheel = self.ttl_wheel.write().await;
                        ttl_wheel.add_key(key, ttl_seconds);
                    }
                    Response::Ok
                } else {
                    Response::Error("Memory limit exceeded".to_string())
                }
            }
            Command::Get { key } => {
                debug!("Shard {}: GET {}", self.id, String::from_utf8_lossy(&key));
                let mut store = self.store.write().await;
                match store.get(&key) {
                    Some(value) => Response::Value(value),
                    None => Response::Null,
                }
            }
            Command::Del { key } => {
                debug!("Shard {}: DEL {}", self.id, String::from_utf8_lossy(&key));
                let mut store = self.store.write().await;
                let mut ttl_wheel = self.ttl_wheel.write().await;
                
                // Remove from both store and TTL wheel
                ttl_wheel.remove_key(&key);
                if store.del(&key) {
                    Response::Ok
                } else {
                    Response::Null // Key didn't exist
                }
            }
            Command::Expire { key, ttl } => {
                debug!("Shard {}: EXPIRE {} ttl={}", self.id, String::from_utf8_lossy(&key), ttl);
                let mut store = self.store.write().await;
                let mut ttl_wheel = self.ttl_wheel.write().await;
                
                if store.expire(&key, ttl) {
                    // Update TTL wheel
                    ttl_wheel.add_key(key, ttl);
                    Response::Ok
                } else {
                    Response::Null // Key didn't exist
                }
            }
            Command::Stats => {
                let store = self.store.read().await;
                let ttl_wheel = self.ttl_wheel.read().await;
                
                let (memory_used, max_memory, key_count) = store.memory_stats();
                let (ttl_keys, ttl_slots) = ttl_wheel.stats();
                
                let stats = format!("shard_{}: {} keys, {}B/{}B memory, {} TTL keys in {} slots", 
                                   self.id, key_count, memory_used, max_memory, ttl_keys, ttl_slots);
                Response::Stats(stats)
            }
            Command::Metrics => {
                // Return basic shard metrics (placeholder for now)
                let store = self.store.read().await;
                let (memory_used, max_memory, key_count) = store.memory_stats();
                Response::Stats(format!("shard_{}_metrics: keys={}, memory={}B/{}B", 
                                       self.id, key_count, memory_used, max_memory))
            }
            Command::Ping => Response::Pong,
        }
    }
    
    /// Get shard statistics
    pub async fn get_stats(&self) -> (usize, usize, usize, usize) {
        let store = self.store.read().await;
        let (memory_used, max_memory, key_count) = store.memory_stats();
        (self.id, key_count, memory_used, max_memory)
    }
    
    /// Background cleanup of expired items
    pub async fn cleanup_expired(&self) -> usize {
        let mut store = self.store.write().await;
        store.cleanup_expired()
    }
}