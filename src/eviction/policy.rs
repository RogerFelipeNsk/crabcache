//! Eviction policy configuration and types

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for eviction policies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvictionConfig {
    /// Proportion of cache dedicated to Window LRU (0.0 to 1.0)
    pub window_ratio: f64,

    /// Width of Count-Min Sketch (number of buckets)
    pub sketch_width: usize,

    /// Depth of Count-Min Sketch (number of hash functions)
    pub sketch_depth: usize,

    /// Memory threshold to start eviction (0.0 to 1.0)
    pub memory_high_watermark: f64,

    /// Memory threshold to stop eviction (0.0 to 1.0)
    pub memory_low_watermark: f64,

    /// Interval to reset frequency sketch (in seconds)
    pub reset_interval_secs: u64,

    /// Maximum cache capacity per shard
    pub max_capacity: usize,

    /// Enable/disable TinyLFU algorithm
    pub enabled: bool,

    /// Eviction strategy: "batch" for aggressive batch eviction, "gradual" for item-by-item
    pub eviction_strategy: String,

    /// Batch size for batch eviction (number of items to evict at once)
    pub batch_eviction_size: usize,

    /// Minimum items to keep in cache during aggressive eviction
    pub min_items_threshold: usize,

    /// Frequency threshold multiplier for admission policy (higher = more selective)
    pub admission_threshold_multiplier: f64,

    /// Enable adaptive eviction based on memory pressure
    pub adaptive_eviction: bool,
}

impl Default for EvictionConfig {
    fn default() -> Self {
        Self {
            window_ratio: 0.01,         // 1% for Window LRU
            sketch_width: 1024,         // 1K buckets
            sketch_depth: 4,            // 4 hash functions
            memory_high_watermark: 0.8, // 80% memory usage
            memory_low_watermark: 0.6,  // 60% memory usage
            reset_interval_secs: 3600,  // 1 hour
            max_capacity: 10000,        // 10K items per shard
            enabled: true,
            eviction_strategy: "gradual".to_string(), // Default to gradual eviction
            batch_eviction_size: 100,                 // Evict 100 items at once in batch mode
            min_items_threshold: 10,                  // Keep at least 10 items
            admission_threshold_multiplier: 1.0,      // Standard admission policy
            adaptive_eviction: true,                  // Enable adaptive eviction
        }
    }
}

impl EvictionConfig {
    /// Get reset interval as Duration
    pub fn reset_interval(&self) -> Duration {
        Duration::from_secs(self.reset_interval_secs)
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> Result<(), String> {
        if self.window_ratio < 0.0 || self.window_ratio > 1.0 {
            return Err("window_ratio must be between 0.0 and 1.0".to_string());
        }

        if self.sketch_width == 0 {
            return Err("sketch_width must be greater than 0".to_string());
        }

        if self.sketch_depth == 0 {
            return Err("sketch_depth must be greater than 0".to_string());
        }

        if self.memory_high_watermark <= self.memory_low_watermark {
            return Err(
                "memory_high_watermark must be greater than memory_low_watermark".to_string(),
            );
        }

        if self.memory_high_watermark > 1.0 || self.memory_low_watermark < 0.0 {
            return Err("memory watermarks must be between 0.0 and 1.0".to_string());
        }

        if self.max_capacity == 0 {
            return Err("max_capacity must be greater than 0".to_string());
        }

        if !["batch", "gradual"].contains(&self.eviction_strategy.as_str()) {
            return Err("eviction_strategy must be 'batch' or 'gradual'".to_string());
        }

        if self.batch_eviction_size == 0 {
            return Err("batch_eviction_size must be greater than 0".to_string());
        }

        if self.min_items_threshold >= self.max_capacity {
            return Err("min_items_threshold must be less than max_capacity".to_string());
        }

        if self.admission_threshold_multiplier < 0.0 {
            return Err("admission_threshold_multiplier must be non-negative".to_string());
        }

        Ok(())
    }

    /// Calculate window cache size based on total capacity
    pub fn window_size(&self) -> usize {
        ((self.max_capacity as f64) * self.window_ratio).max(1.0) as usize
    }

    /// Calculate main cache size based on total capacity
    pub fn main_size(&self) -> usize {
        self.max_capacity - self.window_size()
    }

    /// Check if using batch eviction strategy
    pub fn is_batch_eviction(&self) -> bool {
        self.eviction_strategy == "batch"
    }

    /// Check if using gradual eviction strategy
    pub fn is_gradual_eviction(&self) -> bool {
        self.eviction_strategy == "gradual"
    }

    /// Get effective batch size based on strategy
    pub fn effective_batch_size(&self) -> usize {
        if self.is_batch_eviction() {
            self.batch_eviction_size
        } else {
            1 // Gradual eviction processes one item at a time
        }
    }
}

/// Eviction policy types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvictionPolicyType {
    /// Simple LRU eviction
    LRU,
    /// TinyLFU with Window LRU
    TinyLFU,
    /// No eviction (cache can grow unbounded)
    None,
}

impl Default for EvictionPolicyType {
    fn default() -> Self {
        Self::TinyLFU
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = EvictionConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_window_ratio() {
        let mut config = EvictionConfig::default();
        config.window_ratio = 1.5;
        assert!(config.validate().is_err());

        config.window_ratio = -0.1;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_watermarks() {
        let mut config = EvictionConfig::default();
        config.memory_high_watermark = 0.5;
        config.memory_low_watermark = 0.7;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_window_and_main_sizes() {
        let config = EvictionConfig {
            max_capacity: 1000,
            window_ratio: 0.1,
            ..Default::default()
        };

        assert_eq!(config.window_size(), 100);
        assert_eq!(config.main_size(), 900);
    }

    #[test]
    fn test_minimum_window_size() {
        let config = EvictionConfig {
            max_capacity: 10,
            window_ratio: 0.01, // Would be 0.1, but minimum is 1
            ..Default::default()
        };

        assert_eq!(config.window_size(), 1);
        assert_eq!(config.main_size(), 9);
    }
}
