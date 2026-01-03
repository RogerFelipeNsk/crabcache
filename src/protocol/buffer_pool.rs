//! Optimized Buffer Pool for TOON Protocol
//!
//! This implementation replaces the inefficient 4MB-per-connection buffers
//! with a smart pooling system that provides significant memory and performance benefits.

use bytes::BytesMut;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;
use tracing::debug;

/// Buffer pool with different sized buffers for optimal memory usage
pub struct BufferPool {
    // Different sized buffer pools
    small_buffers: VecDeque<BytesMut>,  // 4KB - for small requests
    medium_buffers: VecDeque<BytesMut>, // 64KB - for typical batches
    large_buffers: VecDeque<BytesMut>,  // 256KB - for large batches
    xlarge_buffers: VecDeque<BytesMut>, // 1MB - for very large operations

    // Pool configuration
    config: BufferPoolConfig,

    // Statistics for monitoring and optimization
    stats: BufferPoolStats,

    // Last cleanup time
    last_cleanup: Instant,
}

/// Configuration for buffer pool
#[derive(Debug, Clone)]
pub struct BufferPoolConfig {
    // Pool sizes (maximum number of buffers to keep)
    pub max_small_buffers: usize,
    pub max_medium_buffers: usize,
    pub max_large_buffers: usize,
    pub max_xlarge_buffers: usize,

    // Buffer sizes
    pub small_buffer_size: usize,
    pub medium_buffer_size: usize,
    pub large_buffer_size: usize,
    pub xlarge_buffer_size: usize,

    // Cleanup configuration
    pub cleanup_interval_secs: u64,
    pub enable_prewarming: bool,
    pub prewarm_count: usize,
}

impl Default for BufferPoolConfig {
    fn default() -> Self {
        Self {
            max_small_buffers: 200,  // 200 * 4KB = 800KB max
            max_medium_buffers: 100, // 100 * 64KB = 6.4MB max
            max_large_buffers: 50,   // 50 * 256KB = 12.8MB max
            max_xlarge_buffers: 20,  // 20 * 1MB = 20MB max

            small_buffer_size: 4 * 1024,     // 4KB
            medium_buffer_size: 64 * 1024,   // 64KB
            large_buffer_size: 256 * 1024,   // 256KB
            xlarge_buffer_size: 1024 * 1024, // 1MB

            cleanup_interval_secs: 60,
            enable_prewarming: true,
            prewarm_count: 10,
        }
    }
}

/// Statistics for buffer pool monitoring
#[derive(Debug, Default)]
pub struct BufferPoolStats {
    // Allocation statistics
    pub total_allocations: AtomicU64,
    pub total_reuses: AtomicU64,
    pub total_returns: AtomicU64,

    // Size-specific statistics
    pub small_allocations: AtomicUsize,
    pub medium_allocations: AtomicUsize,
    pub large_allocations: AtomicUsize,
    pub xlarge_allocations: AtomicUsize,

    pub small_reuses: AtomicUsize,
    pub medium_reuses: AtomicUsize,
    pub large_reuses: AtomicUsize,
    pub xlarge_reuses: AtomicUsize,

    // Memory statistics
    pub total_memory_allocated: AtomicU64,
    pub total_memory_reused: AtomicU64,
    pub peak_memory_usage: AtomicU64,

    // Performance statistics
    pub avg_allocation_time_ns: AtomicU64,
    pub cleanup_count: AtomicUsize,
}

impl BufferPool {
    /// Create new buffer pool with default configuration
    pub fn new() -> Self {
        Self::with_config(BufferPoolConfig::default())
    }

    /// Create new buffer pool with custom configuration
    pub fn with_config(config: BufferPoolConfig) -> Self {
        let mut pool = Self {
            small_buffers: VecDeque::with_capacity(config.max_small_buffers),
            medium_buffers: VecDeque::with_capacity(config.max_medium_buffers),
            large_buffers: VecDeque::with_capacity(config.max_large_buffers),
            xlarge_buffers: VecDeque::with_capacity(config.max_xlarge_buffers),
            config,
            stats: BufferPoolStats::default(),
            last_cleanup: Instant::now(),
        };

        // Pre-warm buffers if enabled
        if pool.config.enable_prewarming {
            pool.prewarm_buffers();
        }

        pool
    }

    /// Get buffer with size hint for optimal allocation
    pub fn get_buffer(&mut self, size_hint: usize) -> BytesMut {
        let start_time = Instant::now();

        // Periodic cleanup
        self.maybe_cleanup();

        let buffer = match size_hint {
            0..=4096 => self.get_small_buffer(),
            4097..=65536 => self.get_medium_buffer(),
            65537..=262144 => self.get_large_buffer(),
            _ => self.get_xlarge_buffer(),
        };

        let allocation_time = start_time.elapsed().as_nanos() as u64;
        self.update_allocation_stats(allocation_time);

        buffer.unwrap_or_else(|| {
            // Fallback: allocate new buffer
            self.allocate_new_buffer(size_hint)
        })
    }

    /// Get buffer optimized for read operations
    pub fn get_read_buffer(&mut self) -> BytesMut {
        // Read operations typically need medium-sized buffers
        self.get_buffer(self.config.medium_buffer_size)
    }

    /// Get buffer optimized for write operations
    pub fn get_write_buffer(&mut self) -> BytesMut {
        // Write operations might need larger buffers for coalescing
        self.get_buffer(self.config.large_buffer_size)
    }

    /// Return buffer to pool for reuse
    pub fn return_buffer(&mut self, mut buffer: BytesMut) {
        // Clear buffer but keep capacity
        buffer.clear();

        let capacity = buffer.capacity();
        let returned = match capacity {
            0..=8192 => {
                if self.small_buffers.len() < self.config.max_small_buffers {
                    self.small_buffers.push_back(buffer);
                    true
                } else {
                    false
                }
            }
            8193..=131072 => {
                if self.medium_buffers.len() < self.config.max_medium_buffers {
                    self.medium_buffers.push_back(buffer);
                    true
                } else {
                    false
                }
            }
            131073..=524288 => {
                if self.large_buffers.len() < self.config.max_large_buffers {
                    self.large_buffers.push_back(buffer);
                    true
                } else {
                    false
                }
            }
            _ => {
                if self.xlarge_buffers.len() < self.config.max_xlarge_buffers {
                    self.xlarge_buffers.push_back(buffer);
                    true
                } else {
                    false
                }
            }
        };

        if returned {
            self.stats.total_returns.fetch_add(1, Ordering::Relaxed);
            self.stats
                .total_memory_reused
                .fetch_add(capacity as u64, Ordering::Relaxed);
        }
    }

    /// Get small buffer (4KB)
    fn get_small_buffer(&mut self) -> Option<BytesMut> {
        if let Some(buffer) = self.small_buffers.pop_front() {
            self.stats.small_reuses.fetch_add(1, Ordering::Relaxed);
            self.stats.total_reuses.fetch_add(1, Ordering::Relaxed);
            Some(buffer)
        } else {
            None
        }
    }

    /// Get medium buffer (64KB)
    fn get_medium_buffer(&mut self) -> Option<BytesMut> {
        if let Some(buffer) = self.medium_buffers.pop_front() {
            self.stats.medium_reuses.fetch_add(1, Ordering::Relaxed);
            self.stats.total_reuses.fetch_add(1, Ordering::Relaxed);
            Some(buffer)
        } else {
            None
        }
    }

    /// Get large buffer (256KB)
    fn get_large_buffer(&mut self) -> Option<BytesMut> {
        if let Some(buffer) = self.large_buffers.pop_front() {
            self.stats.large_reuses.fetch_add(1, Ordering::Relaxed);
            self.stats.total_reuses.fetch_add(1, Ordering::Relaxed);
            Some(buffer)
        } else {
            None
        }
    }

    /// Get extra large buffer (1MB)
    fn get_xlarge_buffer(&mut self) -> Option<BytesMut> {
        if let Some(buffer) = self.xlarge_buffers.pop_front() {
            self.stats.xlarge_reuses.fetch_add(1, Ordering::Relaxed);
            self.stats.total_reuses.fetch_add(1, Ordering::Relaxed);
            Some(buffer)
        } else {
            None
        }
    }

    /// Allocate new buffer when pool is empty
    fn allocate_new_buffer(&mut self, size_hint: usize) -> BytesMut {
        let actual_size = match size_hint {
            0..=4096 => {
                self.stats.small_allocations.fetch_add(1, Ordering::Relaxed);
                self.config.small_buffer_size
            }
            4097..=65536 => {
                self.stats
                    .medium_allocations
                    .fetch_add(1, Ordering::Relaxed);
                self.config.medium_buffer_size
            }
            65537..=262144 => {
                self.stats.large_allocations.fetch_add(1, Ordering::Relaxed);
                self.config.large_buffer_size
            }
            _ => {
                self.stats
                    .xlarge_allocations
                    .fetch_add(1, Ordering::Relaxed);
                size_hint.max(self.config.xlarge_buffer_size)
            }
        };

        self.stats.total_allocations.fetch_add(1, Ordering::Relaxed);
        self.stats
            .total_memory_allocated
            .fetch_add(actual_size as u64, Ordering::Relaxed);

        BytesMut::with_capacity(actual_size)
    }

    /// Pre-warm buffers for better initial performance
    fn prewarm_buffers(&mut self) {
        debug!(
            "Pre-warming buffer pool with {} buffers of each size",
            self.config.prewarm_count
        );

        // Pre-allocate small buffers
        for _ in 0..self.config.prewarm_count {
            let buffer = BytesMut::with_capacity(self.config.small_buffer_size);
            self.small_buffers.push_back(buffer);
        }

        // Pre-allocate medium buffers
        for _ in 0..self.config.prewarm_count {
            let buffer = BytesMut::with_capacity(self.config.medium_buffer_size);
            self.medium_buffers.push_back(buffer);
        }

        // Pre-allocate large buffers
        for _ in 0..(self.config.prewarm_count / 2) {
            let buffer = BytesMut::with_capacity(self.config.large_buffer_size);
            self.large_buffers.push_back(buffer);
        }

        // Pre-allocate extra large buffers
        for _ in 0..(self.config.prewarm_count / 4) {
            let buffer = BytesMut::with_capacity(self.config.xlarge_buffer_size);
            self.xlarge_buffers.push_back(buffer);
        }

        debug!(
            "Buffer pool pre-warmed: {} small, {} medium, {} large, {} xlarge",
            self.small_buffers.len(),
            self.medium_buffers.len(),
            self.large_buffers.len(),
            self.xlarge_buffers.len()
        );
    }

    /// Periodic cleanup to prevent memory bloat
    fn maybe_cleanup(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_cleanup).as_secs() >= self.config.cleanup_interval_secs {
            self.cleanup_excess_buffers();
            self.last_cleanup = now;
        }
    }

    /// Clean up excess buffers to free memory
    fn cleanup_excess_buffers(&mut self) {
        let initial_total = self.total_buffers();

        // Keep only half of max capacity during cleanup
        let target_small = self.config.max_small_buffers / 2;
        let target_medium = self.config.max_medium_buffers / 2;
        let target_large = self.config.max_large_buffers / 2;
        let target_xlarge = self.config.max_xlarge_buffers / 2;

        // Cleanup small buffers
        while self.small_buffers.len() > target_small {
            self.small_buffers.pop_back();
        }

        // Cleanup medium buffers
        while self.medium_buffers.len() > target_medium {
            self.medium_buffers.pop_back();
        }

        // Cleanup large buffers
        while self.large_buffers.len() > target_large {
            self.large_buffers.pop_back();
        }

        // Cleanup extra large buffers
        while self.xlarge_buffers.len() > target_xlarge {
            self.xlarge_buffers.pop_back();
        }

        let final_total = self.total_buffers();
        self.stats.cleanup_count.fetch_add(1, Ordering::Relaxed);

        debug!(
            "Buffer pool cleanup: {} -> {} buffers ({} freed)",
            initial_total,
            final_total,
            initial_total - final_total
        );
    }

    /// Update allocation statistics
    fn update_allocation_stats(&self, allocation_time_ns: u64) {
        // Update average allocation time using exponential moving average
        let current_avg = self.stats.avg_allocation_time_ns.load(Ordering::Relaxed);
        let new_avg = if current_avg == 0 {
            allocation_time_ns
        } else {
            (current_avg * 9 + allocation_time_ns) / 10 // 90% old, 10% new
        };
        self.stats
            .avg_allocation_time_ns
            .store(new_avg, Ordering::Relaxed);

        // Update peak memory usage
        let current_memory = self.calculate_current_memory_usage();
        let peak = self.stats.peak_memory_usage.load(Ordering::Relaxed);
        if current_memory > peak {
            self.stats
                .peak_memory_usage
                .store(current_memory, Ordering::Relaxed);
        }
    }

    /// Calculate current memory usage
    fn calculate_current_memory_usage(&self) -> u64 {
        let small_memory = self.small_buffers.len() as u64 * self.config.small_buffer_size as u64;
        let medium_memory =
            self.medium_buffers.len() as u64 * self.config.medium_buffer_size as u64;
        let large_memory = self.large_buffers.len() as u64 * self.config.large_buffer_size as u64;
        let xlarge_memory =
            self.xlarge_buffers.len() as u64 * self.config.xlarge_buffer_size as u64;

        small_memory + medium_memory + large_memory + xlarge_memory
    }

    /// Get total number of buffers in pool
    fn total_buffers(&self) -> usize {
        self.small_buffers.len()
            + self.medium_buffers.len()
            + self.large_buffers.len()
            + self.xlarge_buffers.len()
    }

    /// Get buffer pool statistics
    pub fn get_stats(&self) -> BufferPoolStatsSnapshot {
        BufferPoolStatsSnapshot {
            total_allocations: self.stats.total_allocations.load(Ordering::Relaxed),
            total_reuses: self.stats.total_reuses.load(Ordering::Relaxed),
            total_returns: self.stats.total_returns.load(Ordering::Relaxed),

            small_allocations: self.stats.small_allocations.load(Ordering::Relaxed),
            medium_allocations: self.stats.medium_allocations.load(Ordering::Relaxed),
            large_allocations: self.stats.large_allocations.load(Ordering::Relaxed),
            xlarge_allocations: self.stats.xlarge_allocations.load(Ordering::Relaxed),

            small_reuses: self.stats.small_reuses.load(Ordering::Relaxed),
            medium_reuses: self.stats.medium_reuses.load(Ordering::Relaxed),
            large_reuses: self.stats.large_reuses.load(Ordering::Relaxed),
            xlarge_reuses: self.stats.xlarge_reuses.load(Ordering::Relaxed),

            total_memory_allocated: self.stats.total_memory_allocated.load(Ordering::Relaxed),
            total_memory_reused: self.stats.total_memory_reused.load(Ordering::Relaxed),
            peak_memory_usage: self.stats.peak_memory_usage.load(Ordering::Relaxed),
            current_memory_usage: self.calculate_current_memory_usage(),

            avg_allocation_time_ns: self.stats.avg_allocation_time_ns.load(Ordering::Relaxed),
            cleanup_count: self.stats.cleanup_count.load(Ordering::Relaxed),

            // Current pool sizes
            small_buffers_available: self.small_buffers.len(),
            medium_buffers_available: self.medium_buffers.len(),
            large_buffers_available: self.large_buffers.len(),
            xlarge_buffers_available: self.xlarge_buffers.len(),

            // Efficiency metrics
            reuse_rate: if self.stats.total_allocations.load(Ordering::Relaxed) > 0 {
                self.stats.total_reuses.load(Ordering::Relaxed) as f64
                    / (self.stats.total_allocations.load(Ordering::Relaxed)
                        + self.stats.total_reuses.load(Ordering::Relaxed))
                        as f64
            } else {
                0.0
            },
        }
    }
}

/// Snapshot of buffer pool statistics
#[derive(Debug, Clone)]
pub struct BufferPoolStatsSnapshot {
    pub total_allocations: u64,
    pub total_reuses: u64,
    pub total_returns: u64,

    pub small_allocations: usize,
    pub medium_allocations: usize,
    pub large_allocations: usize,
    pub xlarge_allocations: usize,

    pub small_reuses: usize,
    pub medium_reuses: usize,
    pub large_reuses: usize,
    pub xlarge_reuses: usize,

    pub total_memory_allocated: u64,
    pub total_memory_reused: u64,
    pub peak_memory_usage: u64,
    pub current_memory_usage: u64,

    pub avg_allocation_time_ns: u64,
    pub cleanup_count: usize,

    pub small_buffers_available: usize,
    pub medium_buffers_available: usize,
    pub large_buffers_available: usize,
    pub xlarge_buffers_available: usize,

    pub reuse_rate: f64,
}

impl std::fmt::Display for BufferPoolStatsSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f,
            "BufferPool: {:.1}% reuse rate, {} allocs, {} reuses, {:.1}MB current, {:.1}MB peak, {:.1}μs avg alloc time",
            self.reuse_rate * 100.0,
            self.total_allocations,
            self.total_reuses,
            self.current_memory_usage as f64 / (1024.0 * 1024.0),
            self.peak_memory_usage as f64 / (1024.0 * 1024.0),
            self.avg_allocation_time_ns as f64 / 1000.0
        )
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}
