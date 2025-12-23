//! Shard storage implementation

use super::Item;
use bytes::Bytes;
use ahash::AHashMap;
use tracing::debug;

/// Storage backend for a shard
pub struct ShardStore {
    items: AHashMap<Bytes, Item>,
    memory_used: usize,
    max_memory: usize,
}

impl ShardStore {
    /// Create a new shard store
    pub fn new(max_memory: usize) -> Self {
        Self {
            items: AHashMap::new(),
            memory_used: 0,
            max_memory,
        }
    }
    
    /// Put an item in the store
    pub fn put(&mut self, key: Bytes, value: Bytes, ttl: Option<u64>) -> bool {
        let item = if let Some(ttl_seconds) = ttl {
            Item::with_ttl(key.clone(), value, ttl_seconds)
        } else {
            Item::new(key.clone(), value, None)
        };
        
        let item_size = item.binary_size();
        
        // Check if we have space (simple check for now)
        if self.memory_used + item_size > self.max_memory && !self.items.contains_key(&key) {
            debug!("Memory limit reached: {} + {} > {}", self.memory_used, item_size, self.max_memory);
            return false;
        }
        
        // Remove old item if exists
        if let Some(old_item) = self.items.remove(&key) {
            self.memory_used = self.memory_used.saturating_sub(old_item.binary_size());
        }
        
        // Add new item
        self.memory_used += item_size;
        self.items.insert(key, item);
        
        debug!("PUT: memory_used = {} / {}", self.memory_used, self.max_memory);
        true
    }
    
    /// Get an item from the store
    pub fn get(&mut self, key: &Bytes) -> Option<Bytes> {
        if let Some(item) = self.items.get(key) {
            // Check if expired
            if item.is_expired() {
                debug!("Item expired, removing: {}", String::from_utf8_lossy(key));
                let expired_item = self.items.remove(key).unwrap();
                self.memory_used = self.memory_used.saturating_sub(expired_item.binary_size());
                return None;
            }
            
            Some(item.value.clone())
        } else {
            None
        }
    }
    
    /// Delete an item from the store
    pub fn del(&mut self, key: &Bytes) -> bool {
        if let Some(item) = self.items.remove(key) {
            self.memory_used = self.memory_used.saturating_sub(item.binary_size());
            debug!("DEL: memory_used = {} / {}", self.memory_used, self.max_memory);
            true
        } else {
            false
        }
    }
    
    /// Set TTL for an existing item
    pub fn expire(&mut self, key: &Bytes, ttl_seconds: u64) -> bool {
        if let Some(item) = self.items.get_mut(key) {
            let expires_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() + ttl_seconds;
            item.expires_at = Some(expires_at);
            true
        } else {
            false
        }
    }
    
    /// Get number of items in store
    pub fn len(&self) -> usize {
        self.items.len()
    }
    
    /// Check if store is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    
    /// Get memory usage statistics
    pub fn memory_stats(&self) -> (usize, usize, usize) {
        (self.memory_used, self.max_memory, self.items.len())
    }
    
    /// Clean up expired items (background cleanup)
    pub fn cleanup_expired(&mut self) -> usize {
        let mut removed_count = 0;
        let mut to_remove = Vec::new();
        
        for (key, item) in &self.items {
            if item.is_expired() {
                to_remove.push(key.clone());
            }
        }
        
        for key in to_remove {
            if let Some(item) = self.items.remove(&key) {
                self.memory_used = self.memory_used.saturating_sub(item.binary_size());
                removed_count += 1;
            }
        }
        
        if removed_count > 0 {
            debug!("Cleaned up {} expired items, memory_used = {} / {}", 
                   removed_count, self.memory_used, self.max_memory);
        }
        
        removed_count
    }
    
    /// Get all keys (for debugging/stats)
    pub fn keys(&self) -> Vec<Bytes> {
        self.items.keys().cloned().collect()
    }
}