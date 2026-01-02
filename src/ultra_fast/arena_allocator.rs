//! Ultra-fast arena allocator for zero-latency memory management

use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Thread-local arena allocator for ultra-fast allocations
pub struct ArenaAllocator {
    chunks: Vec<ArenaChunk>,
    current_chunk: usize,
    total_allocated: AtomicUsize,
}

struct ArenaChunk {
    memory: NonNull<u8>,
    size: usize,
    offset: AtomicUsize,
}

const CHUNK_SIZE: usize = 1024 * 1024; // 1MB chunks
const MAX_CHUNKS: usize = 64; // 64MB total per thread

thread_local! {
    static ARENA: RefCell<ArenaAllocator> = RefCell::new(ArenaAllocator::new());
}

impl ArenaAllocator {
    /// Create new arena allocator
    pub fn new() -> Self {
        Self {
            chunks: Vec::with_capacity(MAX_CHUNKS),
            current_chunk: 0,
            total_allocated: AtomicUsize::new(0),
        }
    }

    /// Allocate memory in arena (ultra-fast, no deallocation)
    #[inline(always)]
    pub fn alloc<T>(&mut self, value: T) -> &'static mut T {
        let layout = Layout::new::<T>();
        let ptr = self.alloc_raw(layout.size(), layout.align());

        unsafe {
            let typed_ptr = ptr.as_ptr() as *mut T;
            std::ptr::write(typed_ptr, value);
            &mut *typed_ptr
        }
    }

    /// Allocate raw memory
    #[inline(always)]
    fn alloc_raw(&mut self, size: usize, align: usize) -> NonNull<u8> {
        // Fast path: try current chunk
        if let Some(chunk) = self.chunks.get(self.current_chunk) {
            if let Some(ptr) = chunk.try_alloc(size, align) {
                self.total_allocated.fetch_add(size, Ordering::Relaxed);
                return ptr;
            }
        }

        // Slow path: allocate new chunk
        self.allocate_new_chunk();

        // Try again with new chunk
        let chunk = &self.chunks[self.current_chunk];
        chunk
            .try_alloc(size, align)
            .unwrap_or_else(|| panic!("Allocation too large for chunk"))
    }

    /// Allocate new chunk
    fn allocate_new_chunk(&mut self) {
        if self.chunks.len() >= MAX_CHUNKS {
            panic!("Arena allocator exhausted");
        }

        let layout = Layout::from_size_align(CHUNK_SIZE, 64).unwrap(); // 64-byte aligned
        let memory = unsafe {
            NonNull::new(alloc(layout)).unwrap_or_else(|| panic!("Failed to allocate arena chunk"))
        };

        let chunk = ArenaChunk {
            memory,
            size: CHUNK_SIZE,
            offset: AtomicUsize::new(0),
        };

        self.chunks.push(chunk);
        self.current_chunk = self.chunks.len() - 1;
    }

    /// Reset arena (for reuse)
    pub fn reset(&mut self) {
        for chunk in &self.chunks {
            chunk.offset.store(0, Ordering::Relaxed);
        }
        self.current_chunk = 0;
        self.total_allocated.store(0, Ordering::Relaxed);
    }

    /// Get total allocated bytes
    pub fn total_allocated(&self) -> usize {
        self.total_allocated.load(Ordering::Relaxed)
    }
}

impl ArenaChunk {
    /// Try to allocate from this chunk
    #[inline(always)]
    fn try_alloc(&self, size: usize, align: usize) -> Option<NonNull<u8>> {
        let current_offset = self.offset.load(Ordering::Relaxed);

        // Align offset
        let aligned_offset = (current_offset + align - 1) & !(align - 1);
        let new_offset = aligned_offset + size;

        if new_offset <= self.size {
            // Try to claim this space atomically
            match self.offset.compare_exchange_weak(
                current_offset,
                new_offset,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => unsafe {
                    Some(NonNull::new_unchecked(
                        self.memory.as_ptr().add(aligned_offset),
                    ))
                },
                Err(_) => None, // Another thread claimed it, try again
            }
        } else {
            None // Chunk is full
        }
    }
}

impl Drop for ArenaAllocator {
    fn drop(&mut self) {
        for chunk in &self.chunks {
            unsafe {
                let layout = Layout::from_size_align_unchecked(chunk.size, 64);
                dealloc(chunk.memory.as_ptr(), layout);
            }
        }
    }
}

/// Global arena allocation functions
#[inline(always)]
pub fn arena_alloc<T>(value: T) -> &'static mut T {
    ARENA.with(|arena| {
        let mut arena = arena.borrow_mut();
        arena.alloc(value)
    })
}

/// Reset thread-local arena
pub fn arena_reset() {
    ARENA.with(|arena| {
        let mut arena = arena.borrow_mut();
        arena.reset();
    });
}

/// Get arena statistics
pub fn arena_stats() -> usize {
    ARENA.with(|arena| {
        let arena = arena.borrow();
        arena.total_allocated()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_allocation() {
        let mut arena = ArenaAllocator::new();

        let value1 = arena.alloc(42u64);
        let value2 = arena.alloc(100u32);

        assert_eq!(*value1, 42);
        assert_eq!(*value2, 100);

        assert!(arena.total_allocated() > 0);
    }

    #[test]
    fn test_arena_reset() {
        let mut arena = ArenaAllocator::new();

        let _value = arena.alloc(42u64);
        assert!(arena.total_allocated() > 0);

        arena.reset();
        assert_eq!(arena.total_allocated(), 0);
    }

    #[test]
    fn test_thread_local_arena() {
        let value = arena_alloc(42u64);
        assert_eq!(*value, 42);

        let stats_before = arena_stats();
        assert!(stats_before > 0);

        arena_reset();
        let stats_after = arena_stats();
        assert_eq!(stats_after, 0);
    }
}
