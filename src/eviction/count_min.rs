//! Count-Min Sketch implementation for frequency estimation
//! 
//! A probabilistic data structure that estimates the frequency of elements
//! in a data stream using minimal memory.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Count-Min Sketch for frequency estimation
#[derive(Debug, Clone)]
pub struct CountMinSketch {
    /// 2D table of counters [depth][width]
    table: Vec<Vec<u32>>,
    /// Hash seeds for each row
    hash_seeds: Vec<u64>,
    /// Width of the sketch (number of buckets per row)
    width: usize,
    /// Depth of the sketch (number of hash functions/rows)
    depth: usize,
    /// Total number of items added
    size: u64,
}

impl CountMinSketch {
    /// Create a new Count-Min Sketch
    /// 
    /// # Arguments
    /// * `width` - Number of buckets per row (affects accuracy)
    /// * `depth` - Number of hash functions/rows (affects accuracy)
    pub fn new(width: usize, depth: usize) -> Self {
        assert!(width > 0, "Width must be greater than 0");
        assert!(depth > 0, "Depth must be greater than 0");
        
        let table = vec![vec![0u32; width]; depth];
        let hash_seeds = (0..depth).map(|i| (i as u64).wrapping_mul(0x9e3779b97f4a7c15)).collect();
        
        Self {
            table,
            hash_seeds,
            width,
            depth,
            size: 0,
        }
    }
    
    /// Increment the count for a key
    pub fn increment(&mut self, key: &str) {
        for (row, &seed) in self.hash_seeds.iter().enumerate() {
            let hash = self.hash_with_seed(key, seed);
            let bucket = (hash as usize) % self.width;
            
            // Prevent overflow
            if self.table[row][bucket] < u32::MAX {
                self.table[row][bucket] += 1;
            }
        }
        
        self.size = self.size.saturating_add(1);
    }
    
    /// Estimate the frequency of a key
    /// Returns the minimum count across all hash functions
    pub fn estimate(&self, key: &str) -> u32 {
        let mut min_count = u32::MAX;
        
        for (row, &seed) in self.hash_seeds.iter().enumerate() {
            let hash = self.hash_with_seed(key, seed);
            let bucket = (hash as usize) % self.width;
            let count = self.table[row][bucket];
            
            if count < min_count {
                min_count = count;
            }
        }
        
        min_count
    }
    
    /// Reset all counters to zero
    pub fn reset(&mut self) {
        for row in &mut self.table {
            for counter in row {
                *counter = 0;
            }
        }
        self.size = 0;
    }
    
    /// Get the total number of items added
    pub fn size(&self) -> u64 {
        self.size
    }
    
    /// Get the width of the sketch
    pub fn width(&self) -> usize {
        self.width
    }
    
    /// Get the depth of the sketch
    pub fn depth(&self) -> usize {
        self.depth
    }
    
    /// Check if the sketch should be reset based on size
    pub fn should_reset(&self, reset_threshold: u64) -> bool {
        self.size >= reset_threshold
    }
    
    /// Hash a key with a specific seed
    fn hash_with_seed(&self, key: &str, seed: u64) -> u64 {
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        key.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Get memory usage in bytes (approximate)
    pub fn memory_usage(&self) -> usize {
        // Table size + hash seeds + metadata
        self.width * self.depth * std::mem::size_of::<u32>() +
        self.depth * std::mem::size_of::<u64>() +
        std::mem::size_of::<Self>()
    }
}

impl Default for CountMinSketch {
    fn default() -> Self {
        Self::new(1024, 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_sketch() {
        let sketch = CountMinSketch::new(100, 4);
        assert_eq!(sketch.width(), 100);
        assert_eq!(sketch.depth(), 4);
        assert_eq!(sketch.size(), 0);
    }

    #[test]
    #[should_panic(expected = "Width must be greater than 0")]
    fn test_invalid_width() {
        CountMinSketch::new(0, 4);
    }

    #[test]
    #[should_panic(expected = "Depth must be greater than 0")]
    fn test_invalid_depth() {
        CountMinSketch::new(100, 0);
    }

    #[test]
    fn test_increment_and_estimate() {
        let mut sketch = CountMinSketch::new(100, 4);
        
        // Initially, estimate should be 0
        assert_eq!(sketch.estimate("key1"), 0);
        
        // After increment, estimate should be at least 1
        sketch.increment("key1");
        assert!(sketch.estimate("key1") >= 1);
        assert_eq!(sketch.size(), 1);
        
        // Multiple increments
        sketch.increment("key1");
        sketch.increment("key1");
        assert!(sketch.estimate("key1") >= 3);
        assert_eq!(sketch.size(), 3);
    }

    #[test]
    fn test_different_keys() {
        let mut sketch = CountMinSketch::new(100, 4);
        
        sketch.increment("key1");
        sketch.increment("key2");
        sketch.increment("key1");
        
        assert!(sketch.estimate("key1") >= 2);
        assert!(sketch.estimate("key2") >= 1);
        assert_eq!(sketch.size(), 3);
    }

    #[test]
    fn test_reset() {
        let mut sketch = CountMinSketch::new(100, 4);
        
        sketch.increment("key1");
        sketch.increment("key2");
        assert!(sketch.estimate("key1") >= 1);
        assert_eq!(sketch.size(), 2);
        
        sketch.reset();
        assert_eq!(sketch.estimate("key1"), 0);
        assert_eq!(sketch.estimate("key2"), 0);
        assert_eq!(sketch.size(), 0);
    }

    #[test]
    fn test_should_reset() {
        let mut sketch = CountMinSketch::new(100, 4);
        
        assert!(!sketch.should_reset(10));
        
        for _ in 0..15 {
            sketch.increment("key");
        }
        
        assert!(sketch.should_reset(10));
    }

    #[test]
    fn test_overflow_protection() {
        let mut sketch = CountMinSketch::new(1, 1); // Small sketch for testing
        
        // Fill up to near max
        for _ in 0..u32::MAX as u64 {
            sketch.increment("key");
            if sketch.estimate("key") == u32::MAX {
                break;
            }
        }
        
        let before_max = sketch.estimate("key");
        sketch.increment("key"); // This should not overflow
        let after_max = sketch.estimate("key");
        
        // Should not overflow
        assert_eq!(before_max, after_max);
    }

    #[test]
    fn test_memory_usage() {
        let sketch = CountMinSketch::new(100, 4);
        let usage = sketch.memory_usage();
        
        // Should be at least the size of the table
        let expected_table_size = 100 * 4 * std::mem::size_of::<u32>();
        assert!(usage >= expected_table_size);
    }

    #[test]
    fn test_hash_consistency() {
        let sketch = CountMinSketch::new(100, 4);
        
        // Same key should hash to same values
        let hash1 = sketch.hash_with_seed("test_key", 12345);
        let hash2 = sketch.hash_with_seed("test_key", 12345);
        assert_eq!(hash1, hash2);
        
        // Different seeds should produce different hashes
        let hash3 = sketch.hash_with_seed("test_key", 54321);
        assert_ne!(hash1, hash3);
    }
}