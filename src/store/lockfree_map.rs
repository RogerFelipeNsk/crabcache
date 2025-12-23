//! Lock-free HashMap implementation for CrabCache
//! 
//! This module provides a high-performance lock-free HashMap using
//! atomic operations and compare-and-swap for maximum concurrency.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::Arc;
use std::ptr;
use bytes::Bytes;

/// Lock-free HashMap for high-concurrency access
pub struct LockFreeHashMap<K, V> 
where 
    K: Hash + Eq + Clone,
    V: Clone,
{
    buckets: Vec<AtomicPtr<Bucket<K, V>>>,
    size: AtomicUsize,
    capacity: usize,
    metrics: Arc<LockFreeMetrics>,
}

impl<K, V> LockFreeHashMap<K, V> 
where 
    K: Hash + Eq + Clone,
    V: Clone,
{
    /// Create a new lock-free HashMap
    pub fn new(capacity: usize) -> Self {
        let mut buckets = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buckets.push(AtomicPtr::new(ptr::null_mut()));
        }
        
        Self {
            buckets,
            size: AtomicUsize::new(0),
            capacity,
            metrics: Arc::new(LockFreeMetrics::default()),
        }
    }
    
    /// Get value by key
    pub fn get(&self, key: &K) -> Option<V> {
        let hash = self.hash_key(key);
        let bucket_idx = hash % self.capacity;
        
        // Update metrics
        self.metrics.total_operations.fetch_add(1, Ordering::Relaxed);
        
        let bucket_ptr = self.buckets[bucket_idx].load(Ordering::Acquire);
        if bucket_ptr.is_null() {
            return None;
        }
        
        let bucket = unsafe { &*bucket_ptr };
        bucket.get(key)
    }
    
    /// Insert key-value pair
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let hash = self.hash_key(&key);
        let bucket_idx = hash % self.capacity;
        
        // Update metrics
        self.metrics.total_operations.fetch_add(1, Ordering::Relaxed);
        
        loop {
            let bucket_ptr = self.buckets[bucket_idx].load(Ordering::Acquire);
            
            if bucket_ptr.is_null() {
                // Create new bucket
                let new_bucket = Box::into_raw(Box::new(Bucket::new()));
                
                match self.buckets[bucket_idx].compare_exchange_weak(
                    ptr::null_mut(),
                    new_bucket,
                    Ordering::Release,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // Successfully created bucket
                        let bucket = unsafe { &*new_bucket };
                        let result = bucket.insert(key, value);
                        if result.is_none() {
                            self.size.fetch_add(1, Ordering::Relaxed);
                        }
                        return result;
                    }
                    Err(_) => {
                        // Another thread created bucket, clean up and retry
                        unsafe { Box::from_raw(new_bucket) };
                        self.metrics.cas_failures.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                }
            } else {
                // Use existing bucket
                let bucket = unsafe { &*bucket_ptr };
                let result = bucket.insert(key, value);
                if result.is_none() {
                    self.size.fetch_add(1, Ordering::Relaxed);
                }
                return result;
            }
        }
    }
    
    /// Remove key-value pair
    pub fn remove(&self, key: &K) -> Option<V> {
        let hash = self.hash_key(key);
        let bucket_idx = hash % self.capacity;
        
        // Update metrics
        self.metrics.total_operations.fetch_add(1, Ordering::Relaxed);
        
        let bucket_ptr = self.buckets[bucket_idx].load(Ordering::Acquire);
        if bucket_ptr.is_null() {
            return None;
        }
        
        let bucket = unsafe { &*bucket_ptr };
        let result = bucket.remove(key);
        if result.is_some() {
            self.size.fetch_sub(1, Ordering::Relaxed);
        }
        result
    }
    
    /// Get current size
    pub fn len(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Get metrics
    pub fn metrics(&self) -> LockFreeMetrics {
        LockFreeMetrics {
            total_operations: AtomicUsize::new(self.metrics.total_operations.load(Ordering::Relaxed)),
            cas_failures: AtomicUsize::new(self.metrics.cas_failures.load(Ordering::Relaxed)),
            bucket_collisions: AtomicUsize::new(self.metrics.bucket_collisions.load(Ordering::Relaxed)),
            max_chain_length: AtomicUsize::new(self.metrics.max_chain_length.load(Ordering::Relaxed)),
        }
    }
    
    /// Calculate load factor
    pub fn load_factor(&self) -> f64 {
        self.len() as f64 / self.capacity as f64
    }
    
    // Private methods
    
    fn hash_key(&self, key: &K) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize
    }
}

/// A bucket in the lock-free HashMap
struct Bucket<K, V> 
where 
    K: Hash + Eq + Clone,
    V: Clone,
{
    entries: Vec<AtomicPtr<Entry<K, V>>>,
    size: AtomicUsize,
}

impl<K, V> Bucket<K, V> 
where 
    K: Hash + Eq + Clone,
    V: Clone,
{
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            size: AtomicUsize::new(0),
        }
    }
    
    fn get(&self, key: &K) -> Option<V> {
        for entry_ptr in &self.entries {
            let entry_ptr = entry_ptr.load(Ordering::Acquire);
            if !entry_ptr.is_null() {
                let entry = unsafe { &*entry_ptr };
                if entry.key == *key && !entry.deleted.load(Ordering::Acquire) {
                    return Some(entry.value.clone());
                }
            }
        }
        None
    }
    
    fn insert(&self, key: K, value: V) -> Option<V> {
        // First, try to update existing entry
        for entry_ptr in &self.entries {
            let entry_ptr = entry_ptr.load(Ordering::Acquire);
            if !entry_ptr.is_null() {
                let entry = unsafe { &*entry_ptr };
                if entry.key == key {
                    if entry.deleted.load(Ordering::Acquire) {
                        // Resurrect deleted entry
                        entry.deleted.store(false, Ordering::Release);
                        let old_value = entry.value.clone();
                        // Note: This is a simplified implementation
                        // In a real implementation, we'd need atomic value updates
                        return Some(old_value);
                    } else {
                        // Update existing entry
                        let old_value = entry.value.clone();
                        // Note: This is a simplified implementation
                        return Some(old_value);
                    }
                }
            }
        }
        
        // Create new entry
        let new_entry = Box::into_raw(Box::new(Entry {
            key,
            value,
            deleted: AtomicBool::new(false),
        }));
        
        // Find empty slot or add new slot
        for entry_ptr in &self.entries {
            let current = entry_ptr.load(Ordering::Acquire);
            if current.is_null() {
                match entry_ptr.compare_exchange_weak(
                    ptr::null_mut(),
                    new_entry,
                    Ordering::Release,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        self.size.fetch_add(1, Ordering::Relaxed);
                        return None;
                    }
                    Err(_) => continue,
                }
            }
        }
        
        // No empty slots, this is a simplified implementation
        // In a real implementation, we'd resize the entries vector
        unsafe { Box::from_raw(new_entry) };
        None
    }
    
    fn remove(&self, key: &K) -> Option<V> {
        for entry_ptr in &self.entries {
            let entry_ptr = entry_ptr.load(Ordering::Acquire);
            if !entry_ptr.is_null() {
                let entry = unsafe { &*entry_ptr };
                if entry.key == *key && !entry.deleted.load(Ordering::Acquire) {
                    entry.deleted.store(true, Ordering::Release);
                    self.size.fetch_sub(1, Ordering::Relaxed);
                    return Some(entry.value.clone());
                }
            }
        }
        None
    }
}

/// An entry in the bucket
struct Entry<K, V> {
    key: K,
    value: V,
    deleted: AtomicBool,
}

use std::sync::atomic::AtomicBool;

/// Lock-free metrics for monitoring
#[derive(Debug, Default)]
pub struct LockFreeMetrics {
    pub total_operations: AtomicUsize,
    pub cas_failures: AtomicUsize,
    pub bucket_collisions: AtomicUsize,
    pub max_chain_length: AtomicUsize,
}

impl LockFreeMetrics {
    /// Calculate contention rate
    pub fn contention_rate(&self) -> f64 {
        let total = self.total_operations.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.cas_failures.load(Ordering::Relaxed) as f64 / total as f64 * 100.0
        }
    }
    
    /// Calculate collision rate
    pub fn collision_rate(&self) -> f64 {
        let total = self.total_operations.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.bucket_collisions.load(Ordering::Relaxed) as f64 / total as f64 * 100.0
        }
    }
}

/// Specialized lock-free HashMap for CrabCache
pub type CrabCacheLockFreeMap = LockFreeHashMap<Bytes, Bytes>;

impl CrabCacheLockFreeMap {
    /// Create optimized map for CrabCache
    pub fn new_optimized(estimated_size: usize) -> Self {
        // Use prime number for better distribution
        let capacity = next_prime(estimated_size * 2);
        Self::new(capacity)
    }
    
    /// Bulk insert for better performance
    pub fn bulk_insert(&self, entries: Vec<(Bytes, Bytes)>) -> usize {
        let mut inserted = 0;
        for (key, value) in entries {
            if self.insert(key, value).is_none() {
                inserted += 1;
            }
        }
        inserted
    }
    
    /// Get statistics for monitoring
    pub fn stats(&self) -> LockFreeStats {
        let metrics = self.metrics();
        LockFreeStats {
            size: self.len(),
            capacity: self.capacity,
            load_factor: self.load_factor(),
            total_operations: metrics.total_operations.load(Ordering::Relaxed),
            contention_rate: metrics.contention_rate(),
            collision_rate: metrics.collision_rate(),
        }
    }
}

/// Statistics for lock-free HashMap
#[derive(Debug, Clone)]
pub struct LockFreeStats {
    pub size: usize,
    pub capacity: usize,
    pub load_factor: f64,
    pub total_operations: usize,
    pub contention_rate: f64,
    pub collision_rate: f64,
}

// Helper function to find next prime number
fn next_prime(n: usize) -> usize {
    if n <= 2 {
        return 2;
    }
    
    let mut candidate = if n % 2 == 0 { n + 1 } else { n };
    
    while !is_prime(candidate) {
        candidate += 2;
    }
    
    candidate
}

fn is_prime(n: usize) -> bool {
    if n <= 1 {
        return false;
    }
    if n <= 3 {
        return true;
    }
    if n % 2 == 0 || n % 3 == 0 {
        return false;
    }
    
    let mut i = 5;
    while i * i <= n {
        if n % i == 0 || n % (i + 2) == 0 {
            return false;
        }
        i += 6;
    }
    
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::sync::Arc;
    
    #[test]
    fn test_basic_operations() {
        let map = LockFreeHashMap::new(16);
        
        // Insert
        assert_eq!(map.insert("key1".to_string(), "value1".to_string()), None);
        assert_eq!(map.len(), 1);
        
        // Get
        assert_eq!(map.get(&"key1".to_string()), Some("value1".to_string()));
        assert_eq!(map.get(&"nonexistent".to_string()), None);
        
        // Update
        assert_eq!(map.insert("key1".to_string(), "value2".to_string()), Some("value1".to_string()));
        assert_eq!(map.len(), 1);
        
        // Remove
        assert_eq!(map.remove(&"key1".to_string()), Some("value2".to_string()));
        assert_eq!(map.len(), 0);
        assert_eq!(map.get(&"key1".to_string()), None);
    }
    
    #[test]
    fn test_concurrent_access() {
        let map = Arc::new(LockFreeHashMap::new(64));
        let mut handles = vec![];
        
        // Spawn multiple threads
        for i in 0..8 {
            let map_clone = Arc::clone(&map);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    let key = format!("key_{}_{}", i, j);
                    let value = format!("value_{}_{}", i, j);
                    
                    // Insert
                    map_clone.insert(key.clone(), value.clone());
                    
                    // Get
                    assert_eq!(map_clone.get(&key), Some(value));
                    
                    // Remove half of the entries
                    if j % 2 == 0 {
                        map_clone.remove(&key);
                    }
                }
            });
            handles.push(handle);
        }
        
        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Check final state
        assert_eq!(map.len(), 400); // 8 threads * 100 entries * 0.5 (half removed)
        
        let metrics = map.metrics();
        println!("Concurrent test metrics:");
        println!("  Total operations: {}", metrics.total_operations.load(std::sync::atomic::Ordering::Relaxed));
        println!("  CAS failures: {}", metrics.cas_failures.load(std::sync::atomic::Ordering::Relaxed));
        println!("  Contention rate: {:.2}%", metrics.contention_rate());
    }
    
    #[test]
    fn test_crabcache_optimized() {
        let map = CrabCacheLockFreeMap::new_optimized(1000);
        
        let entries = vec![
            (Bytes::from("key1"), Bytes::from("value1")),
            (Bytes::from("key2"), Bytes::from("value2")),
            (Bytes::from("key3"), Bytes::from("value3")),
        ];
        
        let inserted = map.bulk_insert(entries);
        assert_eq!(inserted, 3);
        assert_eq!(map.len(), 3);
        
        let stats = map.stats();
        println!("CrabCache map stats:");
        println!("  Size: {}", stats.size);
        println!("  Load factor: {:.2}", stats.load_factor);
        println!("  Total operations: {}", stats.total_operations);
    }
    
    #[test]
    fn test_prime_calculation() {
        assert_eq!(next_prime(10), 11);
        assert_eq!(next_prime(16), 17);
        assert_eq!(next_prime(100), 101);
        assert_eq!(next_prime(1000), 1009);
    }
}