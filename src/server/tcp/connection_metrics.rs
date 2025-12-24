//! Connection and performance metrics

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Metrics for tracking server performance
#[derive(Debug)]
pub struct ConnectionMetrics {
    // Connection metrics
    pub total_connections: AtomicU64,
    pub active_connections: AtomicUsize,
    pub total_commands: AtomicU64,

    // Performance metrics
    pub total_latency_ns: AtomicU64,
    pub min_latency_ns: AtomicU64,
    pub max_latency_ns: AtomicU64,

    // Error metrics
    pub parse_errors: AtomicU64,
    pub processing_errors: AtomicU64,
    pub network_errors: AtomicU64,

    // Buffer pool metrics
    pub buffer_pool_hits: AtomicU64,
    pub buffer_pool_misses: AtomicU64,
}

impl ConnectionMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        Self {
            total_connections: AtomicU64::new(0),
            active_connections: AtomicUsize::new(0),
            total_commands: AtomicU64::new(0),
            total_latency_ns: AtomicU64::new(0),
            min_latency_ns: AtomicU64::new(u64::MAX),
            max_latency_ns: AtomicU64::new(0),
            parse_errors: AtomicU64::new(0),
            processing_errors: AtomicU64::new(0),
            network_errors: AtomicU64::new(0),
            buffer_pool_hits: AtomicU64::new(0),
            buffer_pool_misses: AtomicU64::new(0),
        }
    }

    /// Record a new connection
    pub fn connection_opened(&self) {
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record connection closed
    pub fn connection_closed(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record command processing time
    pub fn record_command_latency(&self, duration: Duration) {
        let latency_ns = duration.as_nanos() as u64;

        self.total_commands.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ns
            .fetch_add(latency_ns, Ordering::Relaxed);

        // Update min latency
        let mut current_min = self.min_latency_ns.load(Ordering::Relaxed);
        while latency_ns < current_min {
            match self.min_latency_ns.compare_exchange_weak(
                current_min,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_min = x,
            }
        }

        // Update max latency
        let mut current_max = self.max_latency_ns.load(Ordering::Relaxed);
        while latency_ns > current_max {
            match self.max_latency_ns.compare_exchange_weak(
                current_max,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }
    }

    /// Record parse error
    pub fn record_parse_error(&self) {
        self.parse_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record processing error
    pub fn record_processing_error(&self) {
        self.processing_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record network error
    pub fn record_network_error(&self) {
        self.network_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record buffer pool hit
    pub fn record_buffer_pool_hit(&self) {
        self.buffer_pool_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record buffer pool miss
    pub fn record_buffer_pool_miss(&self) {
        self.buffer_pool_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current metrics snapshot
    pub fn snapshot(&self) -> MetricsSnapshot {
        let total_commands = self.total_commands.load(Ordering::Relaxed);
        let total_latency_ns = self.total_latency_ns.load(Ordering::Relaxed);

        MetricsSnapshot {
            total_connections: self.total_connections.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            total_commands,
            avg_latency_ns: if total_commands > 0 {
                total_latency_ns / total_commands
            } else {
                0
            },
            min_latency_ns: {
                let min = self.min_latency_ns.load(Ordering::Relaxed);
                if min == u64::MAX {
                    0
                } else {
                    min
                }
            },
            max_latency_ns: self.max_latency_ns.load(Ordering::Relaxed),
            parse_errors: self.parse_errors.load(Ordering::Relaxed),
            processing_errors: self.processing_errors.load(Ordering::Relaxed),
            network_errors: self.network_errors.load(Ordering::Relaxed),
            buffer_pool_hits: self.buffer_pool_hits.load(Ordering::Relaxed),
            buffer_pool_misses: self.buffer_pool_misses.load(Ordering::Relaxed),
        }
    }
}

impl Default for ConnectionMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub total_connections: u64,
    pub active_connections: usize,
    pub total_commands: u64,
    pub avg_latency_ns: u64,
    pub min_latency_ns: u64,
    pub max_latency_ns: u64,
    pub parse_errors: u64,
    pub processing_errors: u64,
    pub network_errors: u64,
    pub buffer_pool_hits: u64,
    pub buffer_pool_misses: u64,
}

impl MetricsSnapshot {
    /// Get average latency in milliseconds
    pub fn avg_latency_ms(&self) -> f64 {
        self.avg_latency_ns as f64 / 1_000_000.0
    }

    /// Get min latency in milliseconds
    pub fn min_latency_ms(&self) -> f64 {
        self.min_latency_ns as f64 / 1_000_000.0
    }

    /// Get max latency in milliseconds
    pub fn max_latency_ms(&self) -> f64 {
        self.max_latency_ns as f64 / 1_000_000.0
    }

    /// Get buffer pool hit rate
    pub fn buffer_pool_hit_rate(&self) -> f64 {
        let total = self.buffer_pool_hits + self.buffer_pool_misses;
        if total > 0 {
            self.buffer_pool_hits as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Get error rate
    pub fn error_rate(&self) -> f64 {
        let total_errors = self.parse_errors + self.processing_errors + self.network_errors;
        if self.total_commands > 0 {
            total_errors as f64 / self.total_commands as f64
        } else {
            0.0
        }
    }
}

/// RAII guard for tracking connection lifetime
pub struct ConnectionGuard {
    metrics: Arc<ConnectionMetrics>,
}

impl ConnectionGuard {
    pub fn new(metrics: Arc<ConnectionMetrics>) -> Self {
        metrics.connection_opened();
        Self { metrics }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.metrics.connection_closed();
    }
}

/// RAII guard for tracking command processing time
pub struct CommandTimer {
    metrics: Arc<ConnectionMetrics>,
    start: Instant,
}

impl CommandTimer {
    pub fn new(metrics: Arc<ConnectionMetrics>) -> Self {
        Self {
            metrics,
            start: Instant::now(),
        }
    }
}

impl Drop for CommandTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        self.metrics.record_command_latency(duration);
    }
}
