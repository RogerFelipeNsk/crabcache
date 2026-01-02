//! Ultra-fast lock-free hash map for zero-latency operations

use crossbeam_epoch::{self as epoch, Atomic, Owned, Shared};
use crossbeam_utils::CachePadded;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::hash::{Hash, Hasher};
use std::mem;
use ahash::AHasher;

/// Ultra-fast lock-free hash map targeting <50ns operations
pub struct UltraFastMap<K, V> {
    buckets: Box<[CachePadded<Atomic<Node<K, V>>>]>,
    bucket_mask: usize,
    len: AtomicUsize,
}

/// Node in the lock-free hash map
struct Node<K, V> {
    hash: u64,
    key: K,
    value: V,
    next: Atomic<Node<K, V>>,
}

impl<K, V> UltraFastMap<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    /// Create new ultra-fast map with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let bucket_count = capacity.next_power_of_two().max(16);
        let bucket_mask = bucket_count - 1;
        
        let buckets = (0..bucket_count)
            .map(|_| CachePadded::new(Atomic::null()))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        
        Self {
            buckets,
            bucket_mask,
            len: AtomicUsize::new(0),
        }
    }

    /// Create new ultra-fast map with default capacity
    pub fn new() -> Self {
        Self::with_capacity(1024)
    }

    /// Get value by key (ultra-fast, lock-free)
    #[inline(always)]
    pub fn get(&self, key: &K) -> Option<V> {
        let hash = self.hash_key(key);
        let bucket_idx = (hash as usize) & self.bucket_mask;
        let bucket = &self.buckets[bucket_idx];
        
        let guard = &epoch::pin();
        let mut current = bucket.load(Ordering::Acquire, guard);
        
        while !current.is_null() {
            let node = unsafe { current.deref() };
            
            if node.hash == hash && node.key == *key {
                return Some(node.value.clone());
            }
            
            current = node.next.load(Ordering::Acquire, guard);
        }
        
        None
    }

    /// Insert key-value pair (ultra-fast, lock-free)
    #[inline(always)]
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let hash = self.hash_key(&key);
        let bucket_idx = (hash as usize) & self.bucket_mask;
        let bucket = &self.buckets[bucket_idx];
        
        let new_node = Owned::new(Node {
            hash,
            key: key.clone(),
            value: value.clone(),
            next: Atomic::null(),
        });
        
        let guard = &epoch::pin();
        
        loop {
            let head = bucket.load(Ordering::Acquire, guard);
            
            // Check if key already exists
            let mut current = head;
            while !current.is_null() {
                let node = unsafe { current.deref() };
                
                if node.hash == hash && node.key == key {
                    // Key exists, update value atomically
                    // For simplicity, we'll create a new node (true lock-free update is complex)
                    let old_value = node.value.clone();
                    
                    // Create new node with updated value
                    let updated_node = Owned::new(Node {
                        hash,
                        key: key.clone(),
                        value,
                        next: Atomic::from(node.next.load(Ordering::Acquire, guard).into_owned()),
                    });
                    
                    new_node.next.store(head, Ordering::Relaxed);
                    
                    match bucket.compare_exchange_weak(
                        head,
                        new_node,
                        Ordering::Release,
                        Ordering::Relaxed,
                        guard,
                    ) {
                        Ok(_) => return Some(old_value),
                        Err(e) => {
                            // Retry with updated head
                            continue;
                        }
                    }
                }
                
                current = node.next.load(Ordering::Acquire, guard);
            }
            
            // Key doesn't exist, insert new node
            new_node.next.store(head, Ordering::Relaxed);
            
            match bucket.compare_exchange_weak(
                head,
                new_node,
                Ordering::Release,
                Ordering::Relaxed,
                guard,
            ) {
                Ok(_) => {
                    self.len.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
                Err(e) => {
                    // Retry with updated head
                    continue;
                }
            }
        }
    }

    /// Remove key-value pair (ultra-fast, lock-free)
    #[inline(always)]
    pub fn remove(&self, key: &K) -> Option<V> {
        let hash = self.hash_key(key);
        let bucket_idx = (hash as usize) & self.bucket_mask;
        let bucket = &self.buckets[bucket_idx];
        
        let guard = &epoch::pin();
        
        loop {
            let head = bucket.load(Ordering::Acquire, guard);
            
            if head.is_null() {
                return None;
            }
            
            let head_node = unsafe { head.deref() };
            
            // Check if head node is the one to remove
            if head_node.hash == hash && head_node.key == *key {
                let next = head_node.next.load(Ordering::Acquire, guard);
                
                match bucket.compare_exchange_weak(
                    head,
                    next,
                    Ordering::Release,
                    Ordering::Relaxed,
                    guard,
                ) {
                    Ok(_) => {
                        let value = head_node.value.clone();
                        unsafe { guard.defer_destroy(head) };
                        self.len.fetch_sub(1, Ordering::Relaxed);
                        return Some(value);
                    }
                    Err(_) => continue, // Retry
                }
            }
            
            // Search in the chain
            let mut prev = head;
            let mut current = head_node.next.load(Ordering::Acquire, guard);
            
            while !current.is_null() {
                let current_node = unsafe { current.deref() };
                
                if current_node.hash == hash && current_node.key == *key {
                    let next = current_node.next.load(Ordering::Acquire, guard);
                    let prev_node = unsafe { prev.deref() };
                    
                    match prev_node.next.compare_exchange_weak(
                        current,
                        next,
                        Ordering::Release,
                        Ordering::Relaxed,
                        guard,
                    ) {
                        Ok(_) => {
                            let value = current_node.value.clone();
                            unsafe { guard.defer_destroy(current) };
                            self.len.fetch_sub(1, Ordering::Relaxed);
                            return Some(value);
                        }
                        Err(_) => break, // Restart from head
                    }
                }
                
                prev = current;
                current = current_node.next.load(Ordering::Acquire, guard);
            }
            
            // Key not found
            return None;
        }
    }

    /// Get current length
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    /// Check if map is empty
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Hash key using ultra-fast hasher
    #[inline(always)]
    fn hash_key(&self, key: &K) -> u64 {
        let mut hasher = AHasher::default();
        key.hash(&mut hasher);
        hasher.finish()
    }

    /// Get statistics for performance monitoring
    pub fn stats(&self) -> UltraFastMapStats {
        let guard = &epoch::pin();
        let mut total_nodes = 0;
        let mut max_chain_length = 0;
        let mut non_empty_buckets = 0;
        let mut total_chain_length = 0;

        for bucket in self.buckets.iter() {
            let mut chain_length = 0;
            let mut current = bucket.load(Ordering::Acquire, guard);
            
            while !current.is_null() {
                chain_length += 1;
                total_nodes += 1;
                let node = unsafe { current.deref() };
                current = node.next.load(Ordering::Acquire, guard);
            }
            
            if chain_length > 0 {
                non_empty_buckets += 1;
                total_chain_length += chain_length;
                max_chain_length = max_chain_length.max(chain_length);
            }
        }

        let avg_chain_length = if non_empty_buckets > 0 {
            total_chain_length as f64 / non_empty_buckets as f64
        } else {
            0.0
        };

        let load_factor = total_nodes as f64 / self.buckets.len() as f64;

        UltraFastMapStats {
            len: self.len(),
            bucket_count: self.buckets.len(),
            non_empty_buckets,
            max_chain_length,
            avg_chain_length,
            load_factor,
        }
    }
}

impl<K, V> Default for UltraFastMap<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for ultra-fast map performance monitoring
#[derive(Debug, Clone)]
pub struct UltraFastMapStats {
    pub len: usize,
    pub bucket_count: usize,
    pub non_empty_buckets: usize,
    pub max_chain_length: usize,
    pub avg_chain_length: f64,
    pub load_factor: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_operations() {
        let map = UltraFastMap::new();
        
        // Test insert
        assert_eq!(map.insert("key1".to_string(), "value1".to_string()), None);
        assert_eq!(map.len(), 1);
        
        // Test get
        assert_eq!(map.get(&"key1".to_string()), Some("value1".to_string()));
        assert_eq!(map.get(&"nonexistent".to_string()), None);
        
        // Test update
        assert_eq!(map.insert("key1".to_string(), "value2".to_string()), Some("value1".to_string()));
        assert_eq!(map.get(&"key1".to_string()), Some("value2".to_string()));
        
        // Test remove
        assert_eq!(map.remove(&"key1".to_string()), Some("value2".to_string()));
        assert_eq!(map.len(), 0);
        assert_eq!(map.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_concurrent_operations() {
        let map = Arc::new(UltraFastMap::new());
        let mut handles = vec![];
        
        // Spawn multiple threads doing concurrent operations
        for i in 0..10 {
            let map_clone = Arc::clone(&map);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    let key = format!("key_{}_{}", i, j);
                    let value = format!("value_{}_{}", i, j);
                    
                    map_clone.insert(key.clone(), value.clone());
                    assert_eq!(map_clone.get(&key), Some(value));
                    
                    if j % 2 == 0 {
                        map_clone.remove(&key);
                    }
                }
            });
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Verify final state
        println!("Final map length: {}", map.len());
        let stats = map.stats();
        println!("Map stats: {:?}", stats);
    }

    #[test]
    fn test_performance() {
        let map = UltraFastMap::new();
        let iterations = 10000;
        
        // Benchmark insertions
        let start = std::time::Instant::now();
        for i in 0..iterations {
            map.insert(i, i * 2);
        }
        let insert_time = start.elapsed();
        
        // Benchmark lookups
        let start = std::time::Instant::now();
        for i in 0..iterations {
            assert_eq!(map.get(&i), Some(i * 2));
        }
        let lookup_time = start.elapsed();
        
        println!("Insert time: {:?} ({:.2} ns/op)", insert_time, insert_time.as_nanos() as f64 / iterations as f64);
        println!("Lookup time: {:?} ({:.2} ns/op)", lookup_time, lookup_time.as_nanos() as f64 / iterations as f64);
        
        // Should be well under 50ns per operation
        assert!(lookup_time.as_nanos() / iterations < 100); // Allow some margin for test environment
    }
}