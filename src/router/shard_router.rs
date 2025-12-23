//! Shard router implementation

use crate::utils::hash::hash_key;
use crate::shard::ShardManager;
use crate::protocol::commands::{Command, Response};
use std::sync::Arc;
use tracing::{debug, info};

/// Routes requests to appropriate shards
pub struct ShardRouter {
    shard_manager: Arc<ShardManager>,
}

impl ShardRouter {
    pub fn new(num_shards: usize, max_memory_per_shard: usize) -> Self {
        info!("Creating ShardRouter with {} shards, {}B per shard", num_shards, max_memory_per_shard);
        let shard_manager = Arc::new(ShardManager::new(num_shards, max_memory_per_shard));
        Self { shard_manager }
    }
    
    /// Route a key to its shard index
    pub fn route_key(&self, key: &[u8]) -> usize {
        let hash = hash_key(key);
        let shard_index = hash as usize % self.shard_manager.num_shards();
        debug!("Routing key {} to shard {}", String::from_utf8_lossy(key), shard_index);
        shard_index
    }
    
    /// Process a command by routing it through the shard manager
    pub async fn process_command(&self, command: Command) -> Response {
        self.shard_manager.process_command(command).await
    }
    
    /// Get shard manager reference
    pub fn shard_manager(&self) -> &Arc<ShardManager> {
        &self.shard_manager
    }
}