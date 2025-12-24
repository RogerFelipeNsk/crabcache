//! TinyLFU cache implementation
//!
//! Combines TinyLFU frequency estimation with Window LRU for newly inserted items
//! and Main LRU for established items. Provides intelligent eviction decisions
//! based on both frequency and recency.

use super::{
    CountMinSketch, EvictionConfig, EvictionMetrics, EvictionPolicy, WindowLRU,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Main LRU cache for established items
#[derive(Debug)]
struct MainLRU {
    items: HashMap<String, Vec<u8>>,
    access_order: Vec<String>,
    max_size: usize,
}

impl MainLRU {
    fn new(max_size: usize) -> Self {
        Self {
            items: HashMap::with_capacity(max_size),
            access_order: Vec::with_capacity(max_size),
            max_size,
        }
    }

    fn get(&mut self, key: &str) -> Option<&Vec<u8>> {
        if self.items.contains_key(key) {
            // Move to end (most recent)
            self.access_order.retain(|k| k != key);
            self.access_order.push(key.to_string());
            self.items.get(key)
        } else {
            None
        }
    }

    fn put(&mut self, key: String, value: Vec<u8>) -> Option<(String, Vec<u8>)> {
        // Check if key already exists
        if self.items.contains_key(&key) {
            // Update existing
            self.items.insert(key.clone(), value);
            // Move to end
            self.access_order.retain(|k| k != &key);
            self.access_order.push(key);
            return None;
        }

        // Add new item
        self.items.insert(key.clone(), value);
        self.access_order.push(key);

        // Check if we need to evict
        if self.items.len() > self.max_size {
            self.remove_lru()
        } else {
            None
        }
    }

    fn remove(&mut self, key: &str) -> Option<Vec<u8>> {
        if let Some(value) = self.items.remove(key) {
            self.access_order.retain(|k| k != key);
            Some(value)
        } else {
            None
        }
    }

    fn remove_lru(&mut self) -> Option<(String, Vec<u8>)> {
        if let Some(lru_key) = self.access_order.first().cloned() {
            if let Some(value) = self.remove(&lru_key) {
                Some((lru_key, value))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn contains_key(&self, key: &str) -> bool {
        self.items.contains_key(key)
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn capacity(&self) -> usize {
        self.max_size
    }

    fn clear(&mut self) {
        self.items.clear();
        self.access_order.clear();
    }
}

/// TinyLFU cache with Window LRU and Main LRU
#[derive(Debug)]
pub struct TinyLFU {
    /// Frequency sketch for admission policy
    frequency_sketch: CountMinSketch,
    /// Window LRU for new items
    window_lru: WindowLRU,
    /// Main LRU for established items
    main_lru: MainLRU,
    /// Configuration
    config: EvictionConfig,
    /// Metrics collection
    metrics: EvictionMetrics,
    /// Last sketch reset time
    last_reset: Instant,
}

impl TinyLFU {
    /// Create a new TinyLFU cache
    pub fn new(config: EvictionConfig) -> Result<Self, String> {
        config.validate()?;

        let window_size = config.window_size();
        let main_size = config.main_size();

        let frequency_sketch = CountMinSketch::new(config.sketch_width, config.sketch_depth);
        let window_lru = WindowLRU::new(window_size);
        let main_lru = MainLRU::new(main_size);
        let metrics = EvictionMetrics::new();

        Ok(Self {
            frequency_sketch,
            window_lru,
            main_lru,
            config,
            metrics,
            last_reset: Instant::now(),
        })
    }

    /// Create with default configuration
    pub fn with_capacity(capacity: usize) -> Self {
        let mut config = EvictionConfig::default();
        config.max_capacity = capacity;
        Self::new(config).expect("Default config should be valid")
    }

    /// Check if an item should be admitted to the cache (improved with threshold multiplier)
    fn should_admit(&self, candidate_key: &str, victim_key: &str) -> bool {
        let candidate_freq = self.frequency_sketch.estimate(candidate_key);
        let victim_freq = self.frequency_sketch.estimate(victim_key);

        // Apply admission threshold multiplier for more selective admission
        let threshold = (victim_freq as f64 * self.config.admission_threshold_multiplier) as u32;

        // Admit if candidate has higher frequency than the adjusted threshold
        candidate_freq >= threshold
    }

    /// Record access to a key in the frequency sketch (optimized)
    fn record_access(&mut self, key: &str) {
        self.frequency_sketch.increment(key);

        // Check if we should reset the sketch (optimized check)
        if self.frequency_sketch.size() > 0 &&
           self.frequency_sketch.size() % 10000 == 0 && // Check every 10k operations
           self.last_reset.elapsed() >= self.config.reset_interval()
        {
            self.reset_sketch();
        }
    }

    /// Reset the frequency sketch
    fn reset_sketch(&mut self) {
        self.frequency_sketch.reset();
        self.last_reset = Instant::now();
        self.metrics.record_sketch_reset();
    }

    /// Promote an item from Window LRU to Main LRU
    fn promote_from_window(&mut self, key: &str) -> bool {
        if let Some(value) = self.window_lru.remove(key) {
            // Try to add to main LRU
            if let Some((evicted_key, _evicted_value)) = self.main_lru.put(key.to_string(), value) {
                // Main LRU was full, check admission policy
                if self.should_admit(key, &evicted_key) {
                    // Keep the new item, evict the old one
                    self.metrics.record_admission_accepted();
                    self.metrics.record_eviction();
                } else {
                    // Reject the new item, restore the old one
                    if let Some(restored_value) = self.main_lru.remove(key) {
                        self.main_lru.put(evicted_key, _evicted_value);
                        // Put rejected item back in window (if space)
                        self.window_lru.put(key.to_string(), restored_value);
                    }
                    self.metrics.record_admission_rejected();
                    return false;
                }
            }

            self.metrics.record_promotion();
            self.update_size_metrics();
            true
        } else {
            false
        }
    }

    /// Update size metrics
    fn update_size_metrics(&self) {
        self.metrics.set_window_size(self.window_lru.len());
        self.metrics.set_main_size(self.main_lru.len());
    }

    /// Force eviction of items to free space (improved with strategy support)
    pub fn evict_items(&mut self, count: usize) -> Vec<(String, Vec<u8>)> {
        let mut evicted = Vec::with_capacity(count);

        // Respect minimum items threshold
        let current_size = self.len();
        if current_size <= self.config.min_items_threshold {
            return evicted; // Don't evict if we're at minimum threshold
        }

        let max_evictable = current_size - self.config.min_items_threshold;
        let actual_count = count.min(max_evictable);

        if self.config.is_batch_eviction() {
            // Batch eviction: remove items in larger batches for better performance
            self.evict_batch(actual_count, &mut evicted);
        } else {
            // Gradual eviction: remove items one by one with more careful selection
            self.evict_gradual(actual_count, &mut evicted);
        }

        self.update_size_metrics();
        evicted
    }

    /// Batch eviction strategy - fast but less precise
    fn evict_batch(&mut self, count: usize, evicted: &mut Vec<(String, Vec<u8>)>) {
        // Prioritize evicting from main LRU first (older, less frequently accessed)
        let main_evict_count = count.min(self.main_lru.len());
        for _ in 0..main_evict_count {
            if let Some(item) = self.main_lru.remove_lru() {
                evicted.push(item);
                self.metrics.record_eviction();
            }
        }

        // If we still need to evict more, evict from window LRU
        let remaining = count - evicted.len();
        for _ in 0..remaining {
            if let Some(item) = self.window_lru.remove_lru() {
                evicted.push(item);
                self.metrics.record_eviction();
            } else {
                break; // No more items to evict
            }
        }
    }

    /// Gradual eviction strategy - slower but more precise
    fn evict_gradual(&mut self, count: usize, evicted: &mut Vec<(String, Vec<u8>)>) {
        // For gradual eviction, we're more selective about what to evict
        // Start with main LRU (established items with lower frequency)
        for _ in 0..count {
            if let Some(item) = self.main_lru.remove_lru() {
                evicted.push(item);
                self.metrics.record_eviction();
            } else if let Some(item) = self.window_lru.remove_lru() {
                evicted.push(item);
                self.metrics.record_eviction();
            } else {
                break; // No more items to evict
            }
        }
    }

    /// Adaptive eviction based on memory pressure
    pub fn adaptive_evict(&mut self, memory_pressure: f64) -> Vec<(String, Vec<u8>)> {
        if !self.config.adaptive_eviction {
            return Vec::new();
        }

        let current_size = self.len();
        if current_size <= self.config.min_items_threshold {
            return Vec::new();
        }

        // Calculate eviction count based on memory pressure
        let pressure_factor = if memory_pressure > self.config.memory_high_watermark {
            // High pressure: more aggressive eviction
            let excess_pressure = memory_pressure - self.config.memory_high_watermark;
            let max_pressure = 1.0 - self.config.memory_high_watermark;
            1.0 + (excess_pressure / max_pressure) * 2.0 // Up to 3x more aggressive
        } else {
            // Normal pressure: standard eviction
            1.0
        };

        let base_evict_count = if self.config.is_batch_eviction() {
            self.config.batch_eviction_size
        } else {
            10 // Gradual eviction with small batches under pressure
        };

        let evict_count = ((base_evict_count as f64) * pressure_factor) as usize;
        let max_evictable = current_size - self.config.min_items_threshold;
        let actual_count = evict_count.min(max_evictable);

        self.evict_items(actual_count)
    }

    /// Batch PUT operations for better performance
    pub fn put_batch(&mut self, items: Vec<(String, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
        let mut all_evicted = Vec::new();

        for (key, value) in items {
            self.record_access(&key);

            // Check if key exists in main LRU
            if self.main_lru.contains_key(&key) {
                self.main_lru.put(key, value);
                continue;
            }

            // Check if key exists in window LRU
            if self.window_lru.contains_key(&key) {
                self.window_lru.put(key, value);
                continue;
            }

            // New item - add to window LRU
            if let Some(evicted) = self.window_lru.put(key.clone(), value) {
                // Try to promote evicted item to main LRU
                let (evicted_key, evicted_value) = evicted;
                if let Some(main_evicted) = self
                    .main_lru
                    .put(evicted_key.clone(), evicted_value.clone())
                {
                    // Main LRU was full, check admission policy
                    if self.should_admit(&evicted_key, &main_evicted.0) {
                        // Accept promotion, evict from main
                        self.metrics.record_promotion();
                        self.metrics.record_eviction();
                        all_evicted.push(main_evicted);
                    } else {
                        // Reject promotion, restore main LRU item
                        self.main_lru.remove(&evicted_key);
                        self.main_lru.put(main_evicted.0.clone(), main_evicted.1);
                        self.metrics.record_admission_rejected();
                        all_evicted.push((evicted_key, evicted_value));
                    }
                } else {
                    // Main LRU had space, promotion successful
                    self.metrics.record_promotion();
                }
            }
        }

        self.update_size_metrics();
        all_evicted
    }
}

impl EvictionPolicy for TinyLFU {
    fn get(&mut self, key: &str) -> Option<Vec<u8>> {
        self.record_access(key);

        // Try window LRU first
        if let Some(value) = self.window_lru.get(key) {
            self.metrics.record_hit();
            return Some(value.clone());
        }

        // Try main LRU
        if let Some(value) = self.main_lru.get(key) {
            self.metrics.record_hit();
            return Some(value.clone());
        }

        self.metrics.record_miss();
        None
    }

    fn put(&mut self, key: String, value: Vec<u8>) -> Option<(String, Vec<u8>)> {
        self.record_access(&key);

        // Check if key exists in main LRU
        if self.main_lru.contains_key(&key) {
            self.main_lru.put(key, value);
            return None;
        }

        // Check if key exists in window LRU
        if self.window_lru.contains_key(&key) {
            self.window_lru.put(key, value);
            return None;
        }

        // New item - add to window LRU
        let evicted = self.window_lru.put(key.clone(), value);

        // If window evicted an item, try to promote it to main
        if let Some((evicted_key, evicted_value)) = evicted {
            // Try to promote evicted item to main LRU
            if let Some(main_evicted) = self
                .main_lru
                .put(evicted_key.clone(), evicted_value.clone())
            {
                // Main LRU was full, check admission policy
                if self.should_admit(&evicted_key, &main_evicted.0) {
                    // Accept promotion, evict from main
                    self.metrics.record_promotion();
                    self.metrics.record_eviction();
                    self.update_size_metrics();
                    return Some(main_evicted);
                } else {
                    // Reject promotion, restore main LRU item
                    self.main_lru.remove(&evicted_key);
                    self.main_lru.put(main_evicted.0.clone(), main_evicted.1);
                    self.metrics.record_admission_rejected();
                    self.update_size_metrics();
                    return Some((evicted_key, evicted_value));
                }
            } else {
                // Main LRU had space, promotion successful
                self.metrics.record_promotion();
            }
        }

        self.update_size_metrics();
        None
    }

    fn remove(&mut self, key: &str) -> Option<Vec<u8>> {
        // Try window LRU first
        if let Some(value) = self.window_lru.remove(key) {
            self.update_size_metrics();
            return Some(value);
        }

        // Try main LRU
        if let Some(value) = self.main_lru.remove(key) {
            self.update_size_metrics();
            return Some(value);
        }

        None
    }

    fn contains_key(&self, key: &str) -> bool {
        self.window_lru.contains_key(key) || self.main_lru.contains_key(key)
    }

    fn len(&self) -> usize {
        self.window_lru.len() + self.main_lru.len()
    }

    fn capacity(&self) -> usize {
        self.config.max_capacity
    }

    fn metrics(&self) -> &EvictionMetrics {
        &self.metrics
    }

    fn reset_metrics(&mut self) {
        self.metrics.reset();
    }

    fn evict_items(&mut self, count: usize) -> Vec<(String, Vec<u8>)> {
        self.evict_items(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config(capacity: usize) -> EvictionConfig {
        EvictionConfig {
            max_capacity: capacity,
            window_ratio: 0.1, // 10% for window
            sketch_width: 64,
            sketch_depth: 4,
            ..Default::default()
        }
    }

    #[test]
    fn test_new_tinylfu() {
        let config = create_test_config(100);
        let cache = TinyLFU::new(config).unwrap();

        assert_eq!(cache.capacity(), 100);
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_with_capacity() {
        let cache = TinyLFU::with_capacity(50);
        assert_eq!(cache.capacity(), 50);
    }

    #[test]
    fn test_basic_put_get() {
        let mut cache = TinyLFU::with_capacity(10);

        // Put and get
        assert_eq!(cache.put("key1".to_string(), b"value1".to_vec()), None);
        assert_eq!(cache.get("key1"), Some(b"value1".to_vec()));
        assert_eq!(cache.len(), 1);

        // Non-existent key
        assert_eq!(cache.get("nonexistent"), None);
    }

    #[test]
    fn test_window_to_main_promotion() {
        let config = EvictionConfig {
            max_capacity: 10,
            window_ratio: 0.2, // 2 items in window, 8 in main
            ..Default::default()
        };
        let mut cache = TinyLFU::new(config).unwrap();

        // Fill window LRU
        cache.put("w1".to_string(), b"value1".to_vec());
        cache.put("w2".to_string(), b"value2".to_vec());

        // This should evict w1 from window and try to promote it to main
        cache.put("w3".to_string(), b"value3".to_vec());

        // w1 should now be in main LRU
        assert_eq!(cache.get("w1"), Some(b"value1".to_vec()));
        assert_eq!(cache.get("w2"), Some(b"value2".to_vec()));
        assert_eq!(cache.get("w3"), Some(b"value3".to_vec()));
    }

    #[test]
    fn test_frequency_based_admission() {
        let mut cache = TinyLFU::with_capacity(4);

        // Add items and access them different numbers of times
        cache.put("freq1".to_string(), b"value1".to_vec());
        cache.put("freq2".to_string(), b"value2".to_vec());
        cache.put("freq3".to_string(), b"value3".to_vec());
        cache.put("freq4".to_string(), b"value4".to_vec());

        // Access freq1 many times to increase its frequency
        for _ in 0..10 {
            cache.get("freq1");
        }

        // Access freq2 a few times
        for _ in 0..3 {
            cache.get("freq2");
        }

        // Fill cache to capacity
        cache.put("new1".to_string(), b"new1".to_vec());
        cache.put("new2".to_string(), b"new2".to_vec());

        // freq1 should still be in cache due to high frequency
        assert_eq!(cache.get("freq1"), Some(b"value1".to_vec()));
    }

    #[test]
    fn test_remove() {
        let mut cache = TinyLFU::with_capacity(5);

        cache.put("key1".to_string(), b"value1".to_vec());
        cache.put("key2".to_string(), b"value2".to_vec());

        assert_eq!(cache.remove("key1"), Some(b"value1".to_vec()));
        assert_eq!(cache.get("key1"), None);
        assert_eq!(cache.get("key2"), Some(b"value2".to_vec()));
        assert_eq!(cache.len(), 1);

        // Remove non-existent key
        assert_eq!(cache.remove("nonexistent"), None);
    }

    #[test]
    fn test_contains_key() {
        let mut cache = TinyLFU::with_capacity(5);

        assert!(!cache.contains_key("key1"));

        cache.put("key1".to_string(), b"value1".to_vec());
        assert!(cache.contains_key("key1"));

        cache.remove("key1");
        assert!(!cache.contains_key("key1"));
    }

    #[test]
    fn test_metrics() {
        let mut cache = TinyLFU::with_capacity(3);

        // Test hits and misses
        cache.put("key1".to_string(), b"value1".to_vec());
        cache.get("key1"); // hit
        cache.get("nonexistent"); // miss

        let metrics = cache.metrics();
        assert_eq!(metrics.cache_hits(), 1);
        assert_eq!(metrics.cache_misses(), 1);
        assert_eq!(metrics.total_requests(), 2);
        assert_eq!(metrics.hit_ratio(), 0.5);
    }

    #[test]
    fn test_evict_items() {
        let mut cache = TinyLFU::with_capacity(3);

        cache.put("key1".to_string(), b"value1".to_vec());
        cache.put("key2".to_string(), b"value2".to_vec());
        cache.put("key3".to_string(), b"value3".to_vec());

        let evicted = cache.evict_items(2);
        assert_eq!(evicted.len(), 2);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_sketch_reset() {
        let config = EvictionConfig {
            max_capacity: 10,
            reset_interval_secs: 1, // Very short interval
            ..Default::default()
        };
        let mut cache = TinyLFU::new(config).unwrap();

        cache.put("key1".to_string(), b"value1".to_vec());

        // Wait for reset interval
        std::thread::sleep(Duration::from_millis(2));

        // This should trigger a reset
        cache.get("key1");

        let metrics = cache.metrics();
        assert!(metrics.snapshot().sketch_resets > 0);
    }
}
