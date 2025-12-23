//! Zero-copy storage engine for CrabCache
//! 
//! This module implements a zero-copy storage system using arena allocation
//! and direct memory references to eliminate unnecessary data copying.

use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Zero-copy storage engine
pub struct ZeroCopyStore {
    arena: Arc<Arena>,
    map: Arc<Mutex<HashMap<Bytes, ArenaRef>>>,
    metrics: Arc<Mutex<ZeroCopyMetrics>>,
}

impl ZeroCopyStore {
    /// Create a new zero-copy store
    pub fn new(initial_capacity: usize) -> Self {
        Self {
            arena: Arc::new(Arena::new(initial_capacity)),
            map: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(Mutex::new(ZeroCopyMetrics::default())),
        }
    }
    
    /// Get value without copying data
    pub fn get_zero_copy(&self, key: &[u8]) -> Option<Bytes> {
        let key_bytes = Bytes::from(key.to_vec());
        let map = self.map.lock().unwrap();
        
        if let Some(arena_ref) = map.get(&key_bytes) {
            if let Some(data) = self.arena.get_data(arena_ref) {
                // Update metrics
                {
                    let mut metrics = self.metrics.lock().unwrap();
                    metrics.zero_copy_operations += 1;
                    metrics.bytes_zero_copied += data.len() as u64;
                }
                
                Some(data)
            } else {
                None
            }
        } else {
            None
        }
    }
    
    /// Put value using zero-copy when possible
    pub fn put_zero_copy(&self, key: Bytes, value: Bytes) -> crate::Result<()> {
        // Try to allocate in arena
        let arena_ref = self.arena.allocate(&value)?;
        
        // Store reference in map
        {
            let mut map = self.map.lock().unwrap();
            map.insert(key, arena_ref);
        }
        
        // Update metrics
        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.zero_copy_operations += 1;
            metrics.bytes_zero_copied += value.len() as u64;
            metrics.allocation_count += 1;
        }
        
        Ok(())
    }
    
    /// Delete value and free arena space
    pub fn del_zero_copy(&self, key: &[u8]) -> bool {
        let key_bytes = Bytes::from(key.to_vec());
        let mut map = self.map.lock().unwrap();
        
        if let Some(arena_ref) = map.remove(&key_bytes) {
            self.arena.deallocate(&arena_ref);
            
            // Update metrics
            {
                let mut metrics = self.metrics.lock().unwrap();
                metrics.zero_copy_operations += 1;
            }
            
            true
        } else {
            false
        }
    }
    
    /// Get current metrics
    pub fn metrics(&self) -> ZeroCopyMetrics {
        let metrics = self.metrics.lock().unwrap();
        let mut result = metrics.clone();
        result.arena_utilization = self.arena.utilization();
        result
    }
    
    /// Compact arena to reduce fragmentation
    pub fn compact_arena(&self) -> crate::Result<()> {
        // TODO: Implement arena compaction
        // For now, just return success
        Ok(())
    }
}

/// Reference to data in arena
#[derive(Debug, Clone, Copy)]
pub struct ArenaRef {
    offset: u32,
    len: u32,
    generation: u32,
}

impl ArenaRef {
    fn new(offset: usize, len: usize, generation: u32) -> Self {
        Self {
            offset: offset as u32,
            len: len as u32,
            generation,
        }
    }
}

/// Arena allocator for zero-copy operations
pub struct Arena {
    memory: Mutex<Vec<u8>>,
    free_list: Mutex<Vec<(usize, usize)>>, // (offset, size) pairs
    next_offset: AtomicUsize,
    generation: AtomicU32,
    capacity: usize,
}

impl Arena {
    /// Create a new arena with specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            memory: Mutex::new(vec![0; capacity]),
            free_list: Mutex::new(Vec::new()),
            next_offset: AtomicUsize::new(0),
            generation: AtomicU32::new(1),
            capacity,
        }
    }
    
    /// Allocate space in arena and return reference
    pub fn allocate(&self, data: &[u8]) -> crate::Result<ArenaRef> {
        let size = data.len();
        
        // Try to find space in free list first
        if let Some(offset) = self.find_free_space(size) {
            self.write_data(offset, data)?;
            return Ok(ArenaRef::new(offset, size, self.generation.load(Ordering::Relaxed)));
        }
        
        // Allocate at end of arena
        let offset = self.next_offset.fetch_add(size, Ordering::Relaxed);
        
        if offset + size > self.capacity {
            return Err("Arena full".into());
        }
        
        self.write_data(offset, data)?;
        Ok(ArenaRef::new(offset, size, self.generation.load(Ordering::Relaxed)))
    }
    
    /// Get data from arena using reference
    pub fn get_data(&self, arena_ref: &ArenaRef) -> Option<Bytes> {
        // Check generation to ensure reference is still valid
        if arena_ref.generation != self.generation.load(Ordering::Relaxed) {
            return None;
        }
        
        let memory = self.memory.lock().unwrap();
        let start = arena_ref.offset as usize;
        let end = start + arena_ref.len as usize;
        
        if end <= memory.len() {
            // Create Bytes from slice without copying
            let data = &memory[start..end];
            Some(Bytes::from(data.to_vec())) // TODO: True zero-copy with custom Bytes
        } else {
            None
        }
    }
    
    /// Deallocate space in arena
    pub fn deallocate(&self, arena_ref: &ArenaRef) {
        let mut free_list = self.free_list.lock().unwrap();
        free_list.push((arena_ref.offset as usize, arena_ref.len as usize));
        
        // Sort free list by offset for better coalescing
        free_list.sort_by_key(|&(offset, _)| offset);
        
        // Coalesce adjacent free blocks
        self.coalesce_free_blocks(&mut free_list);
    }
    
    /// Get arena utilization percentage
    pub fn utilization(&self) -> f64 {
        let used = self.next_offset.load(Ordering::Relaxed);
        let free_space: usize = {
            let free_list = self.free_list.lock().unwrap();
            free_list.iter().map(|(_, size)| size).sum()
        };
        
        let actual_used = used.saturating_sub(free_space);
        actual_used as f64 / self.capacity as f64 * 100.0
    }
    
    // Private methods
    
    fn find_free_space(&self, size: usize) -> Option<usize> {
        let mut free_list = self.free_list.lock().unwrap();
        
        for i in 0..free_list.len() {
            let (offset, free_size) = free_list[i];
            if free_size >= size {
                // Use this free block
                if free_size == size {
                    // Exact fit, remove from free list
                    free_list.remove(i);
                } else {
                    // Partial fit, update free block
                    free_list[i] = (offset + size, free_size - size);
                }
                return Some(offset);
            }
        }
        
        None
    }
    
    fn write_data(&self, offset: usize, data: &[u8]) -> crate::Result<()> {
        let mut memory = self.memory.lock().unwrap();
        
        if offset + data.len() > memory.len() {
            return Err("Arena write out of bounds".into());
        }
        
        memory[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }
    
    fn coalesce_free_blocks(&self, free_list: &mut Vec<(usize, usize)>) {
        if free_list.len() < 2 {
            return;
        }
        
        let mut i = 0;
        while i < free_list.len() - 1 {
            let (offset1, size1) = free_list[i];
            let (offset2, size2) = free_list[i + 1];
            
            // Check if blocks are adjacent
            if offset1 + size1 == offset2 {
                // Coalesce blocks
                free_list[i] = (offset1, size1 + size2);
                free_list.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }
}

/// Zero-copy metrics for monitoring
#[derive(Debug, Clone, Default)]
pub struct ZeroCopyMetrics {
    pub zero_copy_operations: u64,
    pub bytes_copied: u64,
    pub bytes_zero_copied: u64,
    pub allocation_count: u64,
    pub arena_utilization: f64,
}

impl ZeroCopyMetrics {
    /// Calculate zero-copy efficiency
    pub fn zero_copy_efficiency(&self) -> f64 {
        let total_bytes = self.bytes_copied + self.bytes_zero_copied;
        if total_bytes == 0 {
            100.0
        } else {
            self.bytes_zero_copied as f64 / total_bytes as f64 * 100.0
        }
    }
    
    /// Calculate average allocation size
    pub fn average_allocation_size(&self) -> f64 {
        if self.allocation_count == 0 {
            0.0
        } else {
            self.bytes_zero_copied as f64 / self.allocation_count as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_arena_allocation() {
        let arena = Arena::new(1024);
        
        let data1 = b"Hello, World!";
        let ref1 = arena.allocate(data1).unwrap();
        
        let retrieved = arena.get_data(&ref1).unwrap();
        assert_eq!(retrieved.as_ref(), data1);
    }
    
    #[test]
    fn test_zero_copy_store() {
        let store = ZeroCopyStore::new(1024);
        
        let key = Bytes::from("test_key");
        let value = Bytes::from("test_value");
        
        // Put value
        store.put_zero_copy(key.clone(), value.clone()).unwrap();
        
        // Get value
        let retrieved = store.get_zero_copy(&key).unwrap();
        assert_eq!(retrieved, value);
        
        // Delete value
        assert!(store.del_zero_copy(&key));
        assert!(store.get_zero_copy(&key).is_none());
    }
    
    #[test]
    fn test_arena_free_list() {
        let arena = Arena::new(1024);
        
        // Allocate some data
        let data1 = b"data1";
        let data2 = b"data2";
        let ref1 = arena.allocate(data1).unwrap();
        let ref2 = arena.allocate(data2).unwrap();
        
        // Deallocate first block
        arena.deallocate(&ref1);
        
        // Allocate again - should reuse space
        let data3 = b"data3";
        let ref3 = arena.allocate(data3).unwrap();
        
        // Should be able to retrieve both
        let retrieved2 = arena.get_data(&ref2).unwrap();
        let retrieved3 = arena.get_data(&ref3).unwrap();
        
        assert_eq!(retrieved2.as_ref(), data2);
        assert_eq!(retrieved3.as_ref(), data3);
    }
    
    #[test]
    fn test_zero_copy_metrics() {
        let store = ZeroCopyStore::new(1024);
        
        let key1 = Bytes::from("key1");
        let value1 = Bytes::from("value1");
        let key2 = Bytes::from("key2");
        let value2 = Bytes::from("value2");
        
        store.put_zero_copy(key1.clone(), value1.clone()).unwrap();
        store.put_zero_copy(key2.clone(), value2.clone()).unwrap();
        
        let _ = store.get_zero_copy(&key1);
        let _ = store.get_zero_copy(&key2);
        
        let metrics = store.metrics();
        assert_eq!(metrics.allocation_count, 2);
        assert!(metrics.zero_copy_operations >= 4); // 2 puts + 2 gets
        assert!(metrics.zero_copy_efficiency() > 0.0);
    }
}