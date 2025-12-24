//! Window LRU cache implementation
//!
//! A Least Recently Used cache specifically designed for newly inserted items
//! in the TinyLFU algorithm. Items in this cache get a chance to establish
//! their access frequency before competing in the main cache.

use std::collections::{HashMap, VecDeque};

/// Window LRU cache for newly inserted items
#[derive(Debug)]
pub struct WindowLRU {
    /// HashMap for O(1) key lookup
    map: HashMap<String, Vec<u8>>,
    /// Access order queue (most recent at back)
    access_order: VecDeque<String>,
    /// Maximum number of items
    max_size: usize,
}

impl WindowLRU {
    /// Create a new Window LRU cache
    pub fn new(max_size: usize) -> Self {
        assert!(max_size > 0, "Max size must be greater than 0");

        Self {
            map: HashMap::with_capacity(max_size),
            access_order: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    /// Get an item from the cache and mark it as recently used
    pub fn get(&mut self, key: &str) -> Option<&Vec<u8>> {
        if self.map.contains_key(key) {
            // Move to back (most recently used)
            self.move_to_back(key);
            self.map.get(key)
        } else {
            None
        }
    }

    /// Put an item into the cache
    /// Returns the evicted item if cache was full
    pub fn put(&mut self, key: String, value: Vec<u8>) -> Option<(String, Vec<u8>)> {
        // Check if key already exists
        if self.map.contains_key(&key) {
            // Update existing item
            self.map.insert(key.clone(), value);
            self.move_to_back(&key);
            return None;
        }

        // Add new item
        self.map.insert(key.clone(), value);
        self.access_order.push_back(key);

        // Check if we need to evict
        if self.map.len() > self.max_size {
            self.remove_lru()
        } else {
            None
        }
    }

    /// Remove an item from the cache
    pub fn remove(&mut self, key: &str) -> Option<Vec<u8>> {
        if let Some(value) = self.map.remove(key) {
            // Remove from access order
            self.access_order.retain(|k| k != key);
            Some(value)
        } else {
            None
        }
    }

    /// Remove the least recently used item
    pub fn remove_lru(&mut self) -> Option<(String, Vec<u8>)> {
        if let Some(lru_key) = self.access_order.pop_front() {
            if let Some(value) = self.map.remove(&lru_key) {
                Some((lru_key, value))
            } else {
                // Inconsistent state, try next item
                self.remove_lru()
            }
        } else {
            None
        }
    }

    /// Check if the cache contains a key
    pub fn contains_key(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    /// Get the current number of items
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Check if the cache is full
    pub fn is_full(&self) -> bool {
        self.map.len() >= self.max_size
    }

    /// Get the maximum capacity
    pub fn capacity(&self) -> usize {
        self.max_size
    }

    /// Get all keys in LRU order (most recent last)
    pub fn keys(&self) -> Vec<String> {
        self.access_order.iter().cloned().collect()
    }

    /// Clear all items from the cache
    pub fn clear(&mut self) {
        self.map.clear();
        self.access_order.clear();
    }

    /// Move a key to the back of the access order (most recent)
    fn move_to_back(&mut self, key: &str) {
        // Remove from current position
        self.access_order.retain(|k| k != key);
        // Add to back
        self.access_order.push_back(key.to_string());
    }
}

// WindowLRU is thread-safe when used with proper synchronization
unsafe impl Send for WindowLRU {}
unsafe impl Sync for WindowLRU {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_window_lru() {
        let lru = WindowLRU::new(3);
        assert_eq!(lru.capacity(), 3);
        assert_eq!(lru.len(), 0);
        assert!(lru.is_empty());
        assert!(!lru.is_full());
    }

    #[test]
    #[should_panic(expected = "Max size must be greater than 0")]
    fn test_invalid_size() {
        WindowLRU::new(0);
    }

    #[test]
    fn test_put_and_get() {
        let mut lru = WindowLRU::new(3);

        // Put some items
        assert_eq!(lru.put("key1".to_string(), b"value1".to_vec()), None);
        assert_eq!(lru.put("key2".to_string(), b"value2".to_vec()), None);
        assert_eq!(lru.len(), 2);

        // Get items
        assert_eq!(lru.get("key1"), Some(&b"value1".to_vec()));
        assert_eq!(lru.get("key2"), Some(&b"value2".to_vec()));
        assert_eq!(lru.get("nonexistent"), None);
    }

    #[test]
    fn test_lru_eviction() {
        let mut lru = WindowLRU::new(2);

        // Fill cache
        assert_eq!(lru.put("key1".to_string(), b"value1".to_vec()), None);
        assert_eq!(lru.put("key2".to_string(), b"value2".to_vec()), None);
        assert!(lru.is_full());

        // Add third item, should evict key1
        let evicted = lru.put("key3".to_string(), b"value3".to_vec());
        assert_eq!(evicted, Some(("key1".to_string(), b"value1".to_vec())));
        assert_eq!(lru.len(), 2);

        // key1 should be gone, others should remain
        assert_eq!(lru.get("key1"), None);
        assert_eq!(lru.get("key2"), Some(&b"value2".to_vec()));
        assert_eq!(lru.get("key3"), Some(&b"value3".to_vec()));
    }

    #[test]
    fn test_access_updates_order() {
        let mut lru = WindowLRU::new(2);

        lru.put("key1".to_string(), b"value1".to_vec());
        lru.put("key2".to_string(), b"value2".to_vec());

        // Access key1 to make it most recent
        lru.get("key1");

        // Add key3, should evict key2 (least recent)
        let evicted = lru.put("key3".to_string(), b"value3".to_vec());
        assert_eq!(evicted, Some(("key2".to_string(), b"value2".to_vec())));

        // key1 and key3 should remain
        assert_eq!(lru.get("key1"), Some(&b"value1".to_vec()));
        assert_eq!(lru.get("key3"), Some(&b"value3".to_vec()));
        assert_eq!(lru.get("key2"), None);
    }

    #[test]
    fn test_update_existing_key() {
        let mut lru = WindowLRU::new(2);

        lru.put("key1".to_string(), b"value1".to_vec());
        lru.put("key2".to_string(), b"value2".to_vec());

        // Update existing key
        let evicted = lru.put("key1".to_string(), b"new_value1".to_vec());
        assert_eq!(evicted, None); // No eviction
        assert_eq!(lru.len(), 2);

        // Should have new value
        assert_eq!(lru.get("key1"), Some(&b"new_value1".to_vec()));
    }

    #[test]
    fn test_remove() {
        let mut lru = WindowLRU::new(3);

        lru.put("key1".to_string(), b"value1".to_vec());
        lru.put("key2".to_string(), b"value2".to_vec());

        // Remove existing key
        assert_eq!(lru.remove("key1"), Some(b"value1".to_vec()));
        assert_eq!(lru.len(), 1);
        assert_eq!(lru.get("key1"), None);
        assert_eq!(lru.get("key2"), Some(&b"value2".to_vec()));

        // Remove non-existent key
        assert_eq!(lru.remove("nonexistent"), None);
    }

    #[test]
    fn test_contains_key() {
        let mut lru = WindowLRU::new(2);

        assert!(!lru.contains_key("key1"));

        lru.put("key1".to_string(), b"value1".to_vec());
        assert!(lru.contains_key("key1"));

        lru.remove("key1");
        assert!(!lru.contains_key("key1"));
    }

    #[test]
    fn test_keys_order() {
        let mut lru = WindowLRU::new(3);

        lru.put("key1".to_string(), b"value1".to_vec());
        lru.put("key2".to_string(), b"value2".to_vec());
        lru.put("key3".to_string(), b"value3".to_vec());

        // Should be in insertion order (least recent first)
        assert_eq!(lru.keys(), vec!["key1", "key2", "key3"]);

        // Access key1 to make it most recent
        lru.get("key1");
        assert_eq!(lru.keys(), vec!["key2", "key3", "key1"]);
    }

    #[test]
    fn test_clear() {
        let mut lru = WindowLRU::new(3);

        lru.put("key1".to_string(), b"value1".to_vec());
        lru.put("key2".to_string(), b"value2".to_vec());

        assert_eq!(lru.len(), 2);

        lru.clear();

        assert_eq!(lru.len(), 0);
        assert!(lru.is_empty());
        assert_eq!(lru.get("key1"), None);
        assert_eq!(lru.get("key2"), None);
    }

    #[test]
    fn test_remove_lru() {
        let mut lru = WindowLRU::new(3);

        lru.put("key1".to_string(), b"value1".to_vec());
        lru.put("key2".to_string(), b"value2".to_vec());
        lru.put("key3".to_string(), b"value3".to_vec());

        // key1 should be least recent
        assert_eq!(
            lru.remove_lru(),
            Some(("key1".to_string(), b"value1".to_vec()))
        );
        assert_eq!(lru.len(), 2);

        // Access key2 to make key3 least recent
        lru.get("key2");
        assert_eq!(
            lru.remove_lru(),
            Some(("key3".to_string(), b"value3".to_vec()))
        );
        assert_eq!(lru.len(), 1);

        assert_eq!(
            lru.remove_lru(),
            Some(("key2".to_string(), b"value2".to_vec()))
        );
        assert_eq!(lru.len(), 0);

        // Empty cache
        assert_eq!(lru.remove_lru(), None);
    }
}
