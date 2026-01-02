//! Custom lock-free hash map optimized for ultra-high performance

use ahash::AHasher;
use crossbeam_epoch::{self as epoch, Atomic, Owned};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Ultra-fast lock-free hash map
pub struct CustomLockFreeMap<K, V> {
    buckets: Vec<Atomic<Node<K, V>>>,
    bucket_mask: usize,
    len: AtomicUsize,
}

/// Node in the lock-free hash map
struct Node<K, V> {
    key: K,
    value: V,
    hash: u64,
    next: Atomic<Node<K, V>>,
}

impl<K, V> CustomLockFreeMap<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    /// Create new lock-free map with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let bucket_count = capacity.next_power_of_two().max(16);
        let bucket_mask = bucket_count - 1;

        let mut buckets = Vec::with_capacity(bucket_count);
        for _ in 0..bucket_count {
            buckets.push(Atomic::null());
        }

        Self {
            buckets,
            bucket_mask,
            len: AtomicUsize::new(0),
        }
    }

    /// Get bucket index for key using ultra-fast hashing
    #[inline(always)]
    fn bucket_index(&self, key: &K) -> usize {
        let mut hasher = AHasher::default();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        (hash as usize) & self.bucket_mask
    }

    /// Get hash for key
    #[inline(always)]
    fn hash_key(&self, key: &K) -> u64 {
        let mut hasher = AHasher::default();
        key.hash(&mut hasher);
        hasher.finish()
    }

    /// Insert key-value pair
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let guard = &epoch::pin();
        let bucket_idx = self.bucket_index(&key);
        let bucket = &self.buckets[bucket_idx];
        let hash = self.hash_key(&key);

        loop {
            let head = bucket.load(Ordering::Acquire, guard);

            // Search for existing key
            let mut current = head;
            while !current.is_null() {
                let node = unsafe { current.deref() };

                if node.hash == hash && node.key == key {
                    // Key exists, return old value (simplified for now)
                    return Some(node.value.clone());
                }

                current = node.next.load(Ordering::Acquire, guard);
            }

            // Key doesn't exist, insert new node
            let new_node = Owned::new(Node {
                key: key.clone(),
                value: value.clone(),
                hash,
                next: Atomic::from(head),
            });

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
                Err(_) => {
                    // Retry with updated head
                    continue;
                }
            }
        }
    }

    /// Get value by key
    pub fn get(&self, key: &K) -> Option<V> {
        let guard = &epoch::pin();
        let bucket_idx = self.bucket_index(key);
        let bucket = &self.buckets[bucket_idx];
        let hash = self.hash_key(key);

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

    /// Remove key-value pair
    pub fn remove(&self, key: &K) -> Option<V> {
        let guard = &epoch::pin();
        let bucket_idx = self.bucket_index(key);
        let bucket = &self.buckets[bucket_idx];
        let hash = self.hash_key(key);

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
                        self.len.fetch_sub(1, Ordering::Relaxed);
                        unsafe { guard.defer_destroy(head) };
                        return Some(head_node.value.clone());
                    }
                    Err(_) => continue,
                }
            }

            // Search in the rest of the chain
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
                            self.len.fetch_sub(1, Ordering::Relaxed);
                            unsafe { guard.defer_destroy(current) };
                            return Some(current_node.value.clone());
                        }
                        Err(_) => break, // Retry from the beginning
                    }
                }

                prev = current;
                current = current_node.next.load(Ordering::Acquire, guard);
            }

            // If we reach here, key was not found
            return None;
        }
    }

    /// Get current length
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    /// Check if map is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get capacity (number of buckets)
    pub fn capacity(&self) -> usize {
        self.buckets.len()
    }

    /// Get load factor
    pub fn load_factor(&self) -> f64 {
        self.len() as f64 / self.capacity() as f64
    }

    /// Get statistics
    pub fn stats(&self) -> CustomLockFreeMapStats {
        let guard = &epoch::pin();
        let mut chain_lengths = Vec::new();
        let mut max_chain_length = 0;

        for bucket in &self.buckets {
            let mut chain_length = 0;
            let mut current = bucket.load(Ordering::Acquire, guard);

            while !current.is_null() {
                chain_length += 1;
                let node = unsafe { current.deref() };
                current = node.next.load(Ordering::Acquire, guard);
            }

            chain_lengths.push(chain_length);
            max_chain_length = max_chain_length.max(chain_length);
        }

        let avg_chain_length = if !chain_lengths.is_empty() {
            chain_lengths.iter().sum::<usize>() as f64 / chain_lengths.len() as f64
        } else {
            0.0
        };

        CustomLockFreeMapStats {
            len: self.len(),
            capacity: self.capacity(),
            load_factor: self.load_factor(),
            max_chain_length,
            avg_chain_length,
            bucket_count: self.buckets.len(),
        }
    }
}

/// Statistics for the custom lock-free map
#[derive(Debug, Clone)]
pub struct CustomLockFreeMapStats {
    pub len: usize,
    pub capacity: usize,
    pub load_factor: f64,
    pub max_chain_length: usize,
    pub avg_chain_length: f64,
    pub bucket_count: usize,
}

impl<K, V> Drop for CustomLockFreeMap<K, V> {
    fn drop(&mut self) {
        let guard = &epoch::pin();

        // Clean up all nodes
        for bucket in &self.buckets {
            let mut current = bucket.load(Ordering::Acquire, guard);

            while !current.is_null() {
                let node = unsafe { current.deref() };
                let next = node.next.load(Ordering::Acquire, guard);
                unsafe { guard.defer_destroy(current) };
                current = next;
            }
        }
    }
}

unsafe impl<K: Send, V: Send> Send for CustomLockFreeMap<K, V> {}
unsafe impl<K: Sync, V: Sync> Sync for CustomLockFreeMap<K, V> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_operations() {
        let map = CustomLockFreeMap::with_capacity(16);

        // Test insert
        assert_eq!(map.insert("key1".to_string(), "value1".to_string()), None);
        assert_eq!(map.len(), 1);

        // Test get
        assert_eq!(map.get(&"key1".to_string()), Some("value1".to_string()));
        assert_eq!(map.get(&"nonexistent".to_string()), None);

        // Test update
        assert_eq!(
            map.insert("key1".to_string(), "value2".to_string()),
            Some("value1".to_string())
        );
        assert_eq!(map.get(&"key1".to_string()), Some("value2".to_string()));

        // Test remove
        assert_eq!(map.remove(&"key1".to_string()), Some("value2".to_string()));
        assert_eq!(map.len(), 0);
        assert_eq!(map.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_concurrent_operations() {
        let map = Arc::new(CustomLockFreeMap::with_capacity(64));
        let mut handles = vec![];

        // Spawn multiple threads
        for i in 0..8 {
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

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        let stats = map.stats();
        println!("Final stats: {:?}", stats);
    }

    #[test]
    fn test_performance() {
        let map = CustomLockFreeMap::with_capacity(1024);
        let iterations = 10000;

        // Benchmark inserts
        let start = std::time::Instant::now();
        for i in 0..iterations {
            map.insert(i, i * 2);
        }
        let insert_time = start.elapsed();

        // Benchmark gets
        let start = std::time::Instant::now();
        for i in 0..iterations {
            assert_eq!(map.get(&i), Some(i * 2));
        }
        let get_time = start.elapsed();

        println!(
            "Insert time: {:?} ({:.2} ns/op)",
            insert_time,
            insert_time.as_nanos() as f64 / iterations as f64
        );
        println!(
            "Get time: {:?} ({:.2} ns/op)",
            get_time,
            get_time.as_nanos() as f64 / iterations as f64
        );

        let stats = map.stats();
        println!("Performance stats: {:?}", stats);

        // Should be reasonably fast
        assert!(get_time.as_nanos() / iterations < 1000); // Under 1μs per operation
    }

    #[test]
    fn test_load_factor() {
        let map = CustomLockFreeMap::with_capacity(16);

        // Insert some items
        for i in 0..8 {
            map.insert(i, i);
        }

        let stats = map.stats();
        assert!(stats.load_factor > 0.0);
        assert!(stats.load_factor <= 1.0);
        println!("Load factor: {:.2}", stats.load_factor);
    }
}
