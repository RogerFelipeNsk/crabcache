//! Arena allocator for efficient memory management

use std::alloc::{alloc, dealloc, Layout};
use std::ptr::NonNull;
use tracing::debug;

/// Simple arena allocator for reducing memory fragmentation
pub struct ArenaAllocator {
    /// Total capacity of the arena
    capacity: usize,
    /// Current position in the arena
    position: usize,
    /// Pointer to the allocated memory block
    memory: Option<NonNull<u8>>,
    /// Layout of the allocated memory
    layout: Option<Layout>,
}

impl ArenaAllocator {
    /// Create a new arena allocator with given capacity
    pub fn new(capacity: usize) -> Self {
        debug!("Creating ArenaAllocator with capacity: {} bytes", capacity);

        Self {
            capacity,
            position: 0,
            memory: None,
            layout: None,
        }
    }

    /// Initialize the arena (allocate the memory block)
    pub fn init(&mut self) -> Result<(), String> {
        if self.memory.is_some() {
            return Ok(()); // Already initialized
        }

        let layout = Layout::from_size_align(self.capacity, 8)
            .map_err(|e| format!("Invalid layout: {}", e))?;

        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                return Err("Failed to allocate memory".to_string());
            }

            self.memory = Some(NonNull::new_unchecked(ptr));
            self.layout = Some(layout);
        }

        debug!("ArenaAllocator initialized with {} bytes", self.capacity);
        Ok(())
    }

    /// Allocate memory from the arena
    pub fn allocate(&mut self, size: usize, align: usize) -> Option<NonNull<u8>> {
        if self.memory.is_none() {
            if self.init().is_err() {
                return None;
            }
        }

        // Align the current position
        let aligned_pos = (self.position + align - 1) & !(align - 1);

        if aligned_pos + size > self.capacity {
            debug!(
                "Arena allocation failed: {} + {} > {}",
                aligned_pos, size, self.capacity
            );
            return None; // Not enough space
        }

        unsafe {
            let base_ptr = self.memory.unwrap().as_ptr();
            let allocated_ptr = base_ptr.add(aligned_pos);
            self.position = aligned_pos + size;

            debug!("Arena allocated {} bytes at offset {}", size, aligned_pos);
            Some(NonNull::new_unchecked(allocated_ptr))
        }
    }

    /// Reset the arena (doesn't deallocate, just resets position)
    pub fn reset(&mut self) {
        self.position = 0;
        debug!("Arena reset, position = 0");
    }

    /// Get current memory usage statistics
    pub fn stats(&self) -> (usize, usize, f64) {
        let used = self.position;
        let total = self.capacity;
        let utilization = if total > 0 {
            used as f64 / total as f64
        } else {
            0.0
        };
        (used, total, utilization)
    }

    /// Check if arena has enough space for allocation
    pub fn can_allocate(&self, size: usize, align: usize) -> bool {
        let aligned_pos = (self.position + align - 1) & !(align - 1);
        aligned_pos + size <= self.capacity
    }
}

impl Drop for ArenaAllocator {
    fn drop(&mut self) {
        if let (Some(memory), Some(layout)) = (self.memory, self.layout) {
            unsafe {
                dealloc(memory.as_ptr(), layout);
            }
            debug!("ArenaAllocator deallocated");
        }
    }
}

unsafe impl Send for ArenaAllocator {}
unsafe impl Sync for ArenaAllocator {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_basic() {
        let mut arena = ArenaAllocator::new(1024);

        // Should be able to allocate
        let ptr1 = arena.allocate(64, 8);
        assert!(ptr1.is_some());

        let ptr2 = arena.allocate(128, 8);
        assert!(ptr2.is_some());

        // Check stats
        let (used, total, utilization) = arena.stats();
        assert!(used > 0);
        assert_eq!(total, 1024);
        assert!(utilization > 0.0);
    }

    #[test]
    fn test_arena_overflow() {
        let mut arena = ArenaAllocator::new(100);

        // Should fail to allocate more than capacity
        let ptr = arena.allocate(200, 8);
        assert!(ptr.is_none());
    }

    #[test]
    fn test_arena_reset() {
        let mut arena = ArenaAllocator::new(1024);

        arena.allocate(64, 8);
        let (used_before, _, _) = arena.stats();
        assert!(used_before > 0);

        arena.reset();
        let (used_after, _, _) = arena.stats();
        assert_eq!(used_after, 0);
    }
}
