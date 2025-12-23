//! Eviction policy configuration and types

use std::time::Duration;
use serde::{Deserialize, Serialize};

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
}

impl Default for EvictionConfig {
    fn default() -> Self {
        Self {
            window_ratio: 0.01,        // 1% for Window LRU
            sketch_width: 1024,        // 1K buckets
            sketch_depth: 4,           // 4 hash functions
            memory_high_watermark: 0.8, // 80% memory usage
            memory_low_watermark: 0.6,  // 60% memory usage
            reset_interval_secs: 3600,    // 1 hour
            max_capacity: 10000,       // 10K items per shard
            enabled: true,
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
            return Err("memory_high_watermark must be greater than memory_low_watermark".to_string());
        }
        
        if self.memory_high_watermark > 1.0 || self.memory_low_watermark < 0.0 {
            return Err("memory watermarks must be between 0.0 and 1.0".to_string());
        }
        
        if self.max_capacity == 0 {
            return Err("max_capacity must be greater than 0".to_string());
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