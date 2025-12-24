//! Eviction module for CrabCache
//!
//! This module implements advanced eviction algorithms including TinyLFU
//! for intelligent cache management based on frequency and recency.

pub mod count_min;
pub mod memory_monitor;
pub mod metrics;
pub mod policy;
pub mod tinylfu;
pub mod window_lru;

pub use count_min::CountMinSketch;
pub use memory_monitor::{MemoryPressureCoordinator, MemoryPressureMonitor};
pub use metrics::EvictionMetrics;
pub use policy::EvictionConfig;
pub use tinylfu::TinyLFU;
pub use window_lru::WindowLRU;

use std::time::{Duration, Instant};

/// Trait for eviction policies
pub trait EvictionPolicy: Send + Sync {
    /// Get an item from the cache
    fn get(&mut self, key: &str) -> Option<Vec<u8>>;

    /// Put an item into the cache, returning evicted item if any
    fn put(&mut self, key: String, value: Vec<u8>) -> Option<(String, Vec<u8>)>;

    /// Remove an item from the cache
    fn remove(&mut self, key: &str) -> Option<Vec<u8>>;

    /// Check if cache contains key
    fn contains_key(&self, key: &str) -> bool;

    /// Get current cache size
    fn len(&self) -> usize;

    /// Check if cache is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get cache capacity
    fn capacity(&self) -> usize;

    /// Get eviction metrics
    fn metrics(&self) -> &EvictionMetrics;

    /// Reset eviction metrics
    fn reset_metrics(&mut self);

    /// Force eviction of items to free memory
    fn evict_items(&mut self, count: usize) -> Vec<(String, Vec<u8>)>;
}

/// Cache item with metadata
#[derive(Debug, Clone)]
pub struct CacheItem {
    pub key: String,
    pub value: Vec<u8>,
    pub access_time: Instant,
    pub insert_time: Instant,
    pub access_count: u32,
}

impl CacheItem {
    pub fn new(key: String, value: Vec<u8>) -> Self {
        let now = Instant::now();
        Self {
            key,
            value,
            access_time: now,
            insert_time: now,
            access_count: 1,
        }
    }

    pub fn access(&mut self) {
        self.access_time = Instant::now();
        self.access_count = self.access_count.saturating_add(1);
    }

    pub fn age(&self) -> Duration {
        self.insert_time.elapsed()
    }

    pub fn idle_time(&self) -> Duration {
        self.access_time.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_item_creation() {
        let item = CacheItem::new("test_key".to_string(), b"test_value".to_vec());
        assert_eq!(item.key, "test_key");
        assert_eq!(item.value, b"test_value");
        assert_eq!(item.access_count, 1);
    }

    #[test]
    fn test_cache_item_access() {
        let mut item = CacheItem::new("test_key".to_string(), b"test_value".to_vec());
        let initial_count = item.access_count;

        item.access();

        assert_eq!(item.access_count, initial_count + 1);
    }
}
