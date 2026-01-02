//! High-performance buffer pool for reducing allocations with lock-free design

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Lock-free pool of reusable buffers to reduce allocations with performance optimizations
pub struct BufferPool {
    buffer_size: usize,
    max_pool_size: usize,
    // Performance metrics
    hits: AtomicUsize,
    misses: AtomicUsize,
    returns: AtomicUsize,
}

// Thread-local buffer storage for lock-free access
thread_local! {
    static THREAD_LOCAL_BUFFERS: RefCell<VecDeque<Vec<u8>>> = RefCell::new(VecDeque::new());
}

impl BufferPool {
    /// Create a new buffer pool with performance tracking
    pub fn new(buffer_size: usize, max_pool_size: usize) -> Self {
        Self {
            buffer_size,
            max_pool_size,
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
            returns: AtomicUsize::new(0),
        }
    }

    /// Get a buffer from the pool or create a new one (lock-free)
    pub async fn get_buffer(&self) -> Vec<u8> {
        // Use thread-local storage for lock-free access
        let buffer = THREAD_LOCAL_BUFFERS.with(|buffers| {
            let mut buffers = buffers.borrow_mut();

            if let Some(mut buffer) = buffers.pop_front() {
                // Reuse existing buffer (pool hit)
                self.hits.fetch_add(1, Ordering::Relaxed);

                // Fast clear and resize
                buffer.clear();
                buffer.resize(self.buffer_size, 0);
                Some(buffer)
            } else {
                None
            }
        });

        if let Some(buffer) = buffer {
            buffer
        } else {
            // Create new buffer (pool miss)
            self.misses.fetch_add(1, Ordering::Relaxed);

            // Pre-allocate with exact capacity to avoid reallocations
            let mut buffer = Vec::with_capacity(self.buffer_size);
            buffer.resize(self.buffer_size, 0);
            buffer
        }
    }

    /// Return a buffer to the pool (lock-free)
    pub async fn return_buffer(&self, buffer: Vec<u8>) {
        self.returns.fetch_add(1, Ordering::Relaxed);

        // Fast path: check if we should keep the buffer
        if buffer.capacity() < self.buffer_size {
            // Buffer too small, drop it
            return;
        }

        // Use thread-local storage for lock-free access
        THREAD_LOCAL_BUFFERS.with(|buffers| {
            let mut buffers = buffers.borrow_mut();

            // Only keep buffer if pool isn't full
            if buffers.len() < self.max_pool_size {
                buffers.push_back(buffer);
            }
            // Otherwise, let buffer be dropped (automatic memory management)
        });
    }

    /// Get current pool statistics with performance metrics (lock-free)
    pub async fn stats(&self) -> BufferPoolStats {
        let current_size = THREAD_LOCAL_BUFFERS.with(|buffers| buffers.borrow().len());

        BufferPoolStats {
            current_size,
            max_size: self.max_pool_size,
            buffer_size: self.buffer_size,
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            returns: self.returns.load(Ordering::Relaxed),
            hit_rate: self.calculate_hit_rate(),
        }
    }

    /// Calculate hit rate percentage
    fn calculate_hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;

        if total == 0 {
            0.0
        } else {
            (hits as f64 / total as f64) * 100.0
        }
    }

    /// Pre-warm the pool with buffers (useful for startup) - lock-free
    pub async fn prewarm(&self, count: usize) {
        let actual_count = count.min(self.max_pool_size);

        THREAD_LOCAL_BUFFERS.with(|buffers| {
            let mut buffers = buffers.borrow_mut();

            for _ in 0..actual_count {
                let mut buffer = Vec::with_capacity(self.buffer_size);
                buffer.resize(self.buffer_size, 0);
                buffers.push_back(buffer);
            }
        });
    }
}

/// Buffer pool statistics
#[derive(Debug, Clone)]
pub struct BufferPoolStats {
    pub current_size: usize,
    pub max_size: usize,
    pub buffer_size: usize,
    pub hits: usize,
    pub misses: usize,
    pub returns: usize,
    pub hit_rate: f64,
}

impl std::fmt::Display for BufferPoolStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BufferPool: {}/{} buffers ({}B each), {:.1}% hit rate ({} hits, {} misses, {} returns)",
            self.current_size,
            self.max_size,
            self.buffer_size,
            self.hit_rate,
            self.hits,
            self.misses,
            self.returns
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_buffer_pool_reuse() {
        let pool = BufferPool::new(1024, 10);

        // Get a buffer
        let buffer1 = pool.get_buffer().await;
        assert_eq!(buffer1.len(), 1024);

        // Return it
        pool.return_buffer(buffer1).await;

        // Get another buffer (should be reused)
        let buffer2 = pool.get_buffer().await;
        assert_eq!(buffer2.len(), 1024);

        // Check stats
        let stats = pool.stats().await;
        assert_eq!(stats.max_size, 10);
        assert_eq!(stats.hits, 1); // Second get should be a hit
        assert_eq!(stats.misses, 1); // First get should be a miss
    }

    #[tokio::test]
    async fn test_buffer_pool_max_size() {
        let pool = BufferPool::new(1024, 2);

        // Fill the pool
        let buf1 = pool.get_buffer().await;
        let buf2 = pool.get_buffer().await;
        let buf3 = pool.get_buffer().await;

        pool.return_buffer(buf1).await;
        pool.return_buffer(buf2).await;
        pool.return_buffer(buf3).await; // This should be dropped

        let stats = pool.stats().await;
        assert_eq!(stats.current_size, 2); // Only 2 buffers kept
        assert_eq!(stats.returns, 3); // All 3 returns recorded
    }

    #[tokio::test]
    async fn test_buffer_pool_prewarm() {
        let pool = BufferPool::new(1024, 10);

        // Prewarm with 5 buffers
        pool.prewarm(5).await;

        let stats = pool.stats().await;
        assert_eq!(stats.current_size, 5);

        // All gets should be hits now
        for _ in 0..5 {
            let buffer = pool.get_buffer().await;
            assert_eq!(buffer.len(), 1024);
        }

        let stats = pool.stats().await;
        assert_eq!(stats.hits, 5);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.hit_rate, 100.0);
    }

    #[tokio::test]
    async fn test_buffer_pool_stats() {
        let pool = BufferPool::new(2048, 5);

        // Generate some activity
        let buf1 = pool.get_buffer().await; // miss
        let buf2 = pool.get_buffer().await; // miss

        pool.return_buffer(buf1).await;

        let buf3 = pool.get_buffer().await; // hit
        pool.return_buffer(buf2).await;
        pool.return_buffer(buf3).await;

        let stats = pool.stats().await;
        assert_eq!(stats.buffer_size, 2048);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.returns, 3);
        assert!((stats.hit_rate - 33.33).abs() < 0.1); // ~33.33%
    }
}
