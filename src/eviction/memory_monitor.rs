//! Memory pressure monitoring for eviction triggering
//! 
//! Monitors memory usage and triggers eviction when thresholds are exceeded.
//! Supports per-shard monitoring and coordination across multiple shards.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Memory pressure monitor for triggering eviction
#[derive(Debug)]
pub struct MemoryPressureMonitor {
    /// Maximum memory limit in bytes
    memory_limit: usize,
    /// Current memory usage in bytes
    current_usage: AtomicUsize,
    /// High watermark threshold (0.0 to 1.0)
    high_watermark: f64,
    /// Low watermark threshold (0.0 to 1.0)
    low_watermark: f64,
    /// Shard ID for identification
    shard_id: usize,
}

impl MemoryPressureMonitor {
    /// Create a new memory pressure monitor
    pub fn new(
        shard_id: usize,
        memory_limit: usize,
        high_watermark: f64,
        low_watermark: f64,
    ) -> Result<Self, String> {
        if high_watermark <= low_watermark {
            return Err("High watermark must be greater than low watermark".to_string());
        }
        
        if high_watermark > 1.0 || low_watermark < 0.0 {
            return Err("Watermarks must be between 0.0 and 1.0".to_string());
        }
        
        if memory_limit == 0 {
            return Err("Memory limit must be greater than 0".to_string());
        }
        
        Ok(Self {
            memory_limit,
            current_usage: AtomicUsize::new(0),
            high_watermark,
            low_watermark,
            shard_id,
        })
    }
    
    /// Update memory usage by delta (can be positive or negative)
    pub fn update_usage(&self, delta: isize) {
        if delta >= 0 {
            self.current_usage.fetch_add(delta as usize, Ordering::Relaxed);
        } else {
            let abs_delta = (-delta) as usize;
            // Prevent underflow
            self.current_usage.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_sub(abs_delta))
            }).ok();
        }
    }
    
    /// Set absolute memory usage
    pub fn set_usage(&self, usage: usize) {
        self.current_usage.store(usage, Ordering::Relaxed);
    }
    
    /// Get current memory usage
    pub fn current_usage(&self) -> usize {
        self.current_usage.load(Ordering::Relaxed)
    }
    
    /// Get memory limit
    pub fn memory_limit(&self) -> usize {
        self.memory_limit
    }
    
    /// Get usage ratio (0.0 to 1.0+)
    pub fn usage_ratio(&self) -> f64 {
        let current = self.current_usage();
        current as f64 / self.memory_limit as f64
    }
    
    /// Check if eviction should be triggered (above high watermark)
    pub fn should_evict(&self) -> bool {
        self.usage_ratio() >= self.high_watermark
    }
    
    /// Check if eviction should stop (below low watermark)
    pub fn should_stop_eviction(&self) -> bool {
        self.usage_ratio() <= self.low_watermark
    }
    
    /// Get high watermark threshold
    pub fn high_watermark(&self) -> f64 {
        self.high_watermark
    }
    
    /// Get low watermark threshold
    pub fn low_watermark(&self) -> f64 {
        self.low_watermark
    }
    
    /// Get shard ID
    pub fn shard_id(&self) -> usize {
        self.shard_id
    }
    
    /// Calculate bytes above high watermark
    pub fn bytes_over_limit(&self) -> usize {
        let current = self.current_usage();
        let high_limit = (self.memory_limit as f64 * self.high_watermark) as usize;
        current.saturating_sub(high_limit)
    }
    
    /// Calculate how many bytes need to be freed to reach low watermark
    pub fn bytes_to_free(&self) -> usize {
        let current = self.current_usage();
        let low_limit = (self.memory_limit as f64 * self.low_watermark) as usize;
        current.saturating_sub(low_limit)
    }
    
    /// Get memory pressure level (0.0 = no pressure, 1.0+ = over limit)
    pub fn pressure_level(&self) -> f64 {
        let ratio = self.usage_ratio();
        if ratio <= self.low_watermark {
            0.0
        } else if ratio >= self.high_watermark {
            1.0 + (ratio - self.high_watermark)
        } else {
            (ratio - self.low_watermark) / (self.high_watermark - self.low_watermark)
        }
    }
    
    /// Reset memory usage to zero
    pub fn reset(&self) {
        self.current_usage.store(0, Ordering::Relaxed);
    }
}

/// Coordinator for multiple memory pressure monitors
#[derive(Debug)]
pub struct MemoryPressureCoordinator {
    monitors: Vec<Arc<MemoryPressureMonitor>>,
}

impl MemoryPressureCoordinator {
    /// Create a new coordinator
    pub fn new() -> Self {
        Self {
            monitors: Vec::new(),
        }
    }
    
    /// Add a monitor to coordinate
    pub fn add_monitor(&mut self, monitor: Arc<MemoryPressureMonitor>) {
        self.monitors.push(monitor);
    }
    
    /// Get all monitors
    pub fn monitors(&self) -> &[Arc<MemoryPressureMonitor>] {
        &self.monitors
    }
    
    /// Check if any shard needs eviction
    pub fn any_should_evict(&self) -> bool {
        self.monitors.iter().any(|m| m.should_evict())
    }
    
    /// Get shards that need eviction
    pub fn shards_needing_eviction(&self) -> Vec<usize> {
        self.monitors
            .iter()
            .filter(|m| m.should_evict())
            .map(|m| m.shard_id())
            .collect()
    }
    
    /// Get total memory usage across all shards
    pub fn total_usage(&self) -> usize {
        self.monitors.iter().map(|m| m.current_usage()).sum()
    }
    
    /// Get total memory limit across all shards
    pub fn total_limit(&self) -> usize {
        self.monitors.iter().map(|m| m.memory_limit()).sum()
    }
    
    /// Get overall usage ratio
    pub fn overall_usage_ratio(&self) -> f64 {
        let total_usage = self.total_usage();
        let total_limit = self.total_limit();
        
        if total_limit == 0 {
            0.0
        } else {
            total_usage as f64 / total_limit as f64
        }
    }
    
    /// Get the shard with highest pressure
    pub fn highest_pressure_shard(&self) -> Option<usize> {
        self.monitors
            .iter()
            .max_by(|a, b| a.pressure_level().partial_cmp(&b.pressure_level()).unwrap())
            .map(|m| m.shard_id())
    }
    
    /// Get memory statistics for all shards
    pub fn memory_stats(&self) -> Vec<MemoryStats> {
        self.monitors
            .iter()
            .map(|m| MemoryStats {
                shard_id: m.shard_id(),
                current_usage: m.current_usage(),
                memory_limit: m.memory_limit(),
                usage_ratio: m.usage_ratio(),
                pressure_level: m.pressure_level(),
                should_evict: m.should_evict(),
                bytes_to_free: m.bytes_to_free(),
            })
            .collect()
    }
}

impl Default for MemoryPressureCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory statistics for a shard
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub shard_id: usize,
    pub current_usage: usize,
    pub memory_limit: usize,
    pub usage_ratio: f64,
    pub pressure_level: f64,
    pub should_evict: bool,
    pub bytes_to_free: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_monitor() {
        let monitor = MemoryPressureMonitor::new(0, 1000, 0.8, 0.6).unwrap();
        
        assert_eq!(monitor.shard_id(), 0);
        assert_eq!(monitor.memory_limit(), 1000);
        assert_eq!(monitor.current_usage(), 0);
        assert_eq!(monitor.high_watermark(), 0.8);
        assert_eq!(monitor.low_watermark(), 0.6);
        assert_eq!(monitor.usage_ratio(), 0.0);
        assert!(!monitor.should_evict());
    }

    #[test]
    fn test_invalid_watermarks() {
        // High <= Low
        assert!(MemoryPressureMonitor::new(0, 1000, 0.6, 0.8).is_err());
        
        // Out of range
        assert!(MemoryPressureMonitor::new(0, 1000, 1.5, 0.5).is_err());
        assert!(MemoryPressureMonitor::new(0, 1000, 0.8, -0.1).is_err());
        
        // Zero limit
        assert!(MemoryPressureMonitor::new(0, 0, 0.8, 0.6).is_err());
    }

    #[test]
    fn test_update_usage() {
        let monitor = MemoryPressureMonitor::new(0, 1000, 0.8, 0.6).unwrap();
        
        // Positive delta
        monitor.update_usage(100);
        assert_eq!(monitor.current_usage(), 100);
        
        monitor.update_usage(50);
        assert_eq!(monitor.current_usage(), 150);
        
        // Negative delta
        monitor.update_usage(-30);
        assert_eq!(monitor.current_usage(), 120);
        
        // Underflow protection
        monitor.update_usage(-200);
        assert_eq!(monitor.current_usage(), 0);
    }

    #[test]
    fn test_set_usage() {
        let monitor = MemoryPressureMonitor::new(0, 1000, 0.8, 0.6).unwrap();
        
        monitor.set_usage(500);
        assert_eq!(monitor.current_usage(), 500);
        assert_eq!(monitor.usage_ratio(), 0.5);
    }

    #[test]
    fn test_eviction_thresholds() {
        let monitor = MemoryPressureMonitor::new(0, 1000, 0.8, 0.6).unwrap();
        
        // Below low watermark
        monitor.set_usage(500); // 50%
        assert!(!monitor.should_evict());
        assert!(monitor.should_stop_eviction());
        
        // Between watermarks
        monitor.set_usage(700); // 70%
        assert!(!monitor.should_evict());
        assert!(!monitor.should_stop_eviction());
        
        // Above high watermark
        monitor.set_usage(850); // 85%
        assert!(monitor.should_evict());
        assert!(!monitor.should_stop_eviction());
    }

    #[test]
    fn test_bytes_calculations() {
        let monitor = MemoryPressureMonitor::new(0, 1000, 0.8, 0.6).unwrap();
        
        monitor.set_usage(900); // 90%
        
        // High watermark is 800 bytes (80% of 1000)
        assert_eq!(monitor.bytes_over_limit(), 100);
        
        // Low watermark is 600 bytes (60% of 1000)
        assert_eq!(monitor.bytes_to_free(), 300);
    }

    #[test]
    fn test_pressure_level() {
        let monitor = MemoryPressureMonitor::new(0, 1000, 0.8, 0.6).unwrap();
        
        // Below low watermark
        monitor.set_usage(500); // 50%
        assert_eq!(monitor.pressure_level(), 0.0);
        
        // At low watermark
        monitor.set_usage(600); // 60%
        assert_eq!(monitor.pressure_level(), 0.0);
        
        // Between watermarks (70% = halfway between 60% and 80%)
        monitor.set_usage(700); // 70%
        assert_eq!(monitor.pressure_level(), 0.5);
        
        // At high watermark
        monitor.set_usage(800); // 80%
        assert_eq!(monitor.pressure_level(), 1.0);
        
        // Above high watermark
        monitor.set_usage(900); // 90%
        assert_eq!(monitor.pressure_level(), 1.1);
    }

    #[test]
    fn test_coordinator() {
        let mut coordinator = MemoryPressureCoordinator::new();
        
        let monitor1 = Arc::new(MemoryPressureMonitor::new(0, 1000, 0.8, 0.6).unwrap());
        let monitor2 = Arc::new(MemoryPressureMonitor::new(1, 2000, 0.8, 0.6).unwrap());
        
        coordinator.add_monitor(monitor1.clone());
        coordinator.add_monitor(monitor2.clone());
        
        assert_eq!(coordinator.monitors().len(), 2);
        assert_eq!(coordinator.total_limit(), 3000);
        assert_eq!(coordinator.total_usage(), 0);
        assert_eq!(coordinator.overall_usage_ratio(), 0.0);
        assert!(!coordinator.any_should_evict());
    }

    #[test]
    fn test_coordinator_eviction_detection() {
        let mut coordinator = MemoryPressureCoordinator::new();
        
        let monitor1 = Arc::new(MemoryPressureMonitor::new(0, 1000, 0.8, 0.6).unwrap());
        let monitor2 = Arc::new(MemoryPressureMonitor::new(1, 1000, 0.8, 0.6).unwrap());
        
        coordinator.add_monitor(monitor1.clone());
        coordinator.add_monitor(monitor2.clone());
        
        // Set one shard above threshold
        monitor1.set_usage(850); // 85%
        monitor2.set_usage(500); // 50%
        
        assert!(coordinator.any_should_evict());
        assert_eq!(coordinator.shards_needing_eviction(), vec![0]);
        assert_eq!(coordinator.highest_pressure_shard(), Some(0));
    }

    #[test]
    fn test_memory_stats() {
        let mut coordinator = MemoryPressureCoordinator::new();
        
        let monitor = Arc::new(MemoryPressureMonitor::new(0, 1000, 0.8, 0.6).unwrap());
        monitor.set_usage(850);
        coordinator.add_monitor(monitor);
        
        let stats = coordinator.memory_stats();
        assert_eq!(stats.len(), 1);
        
        let stat = &stats[0];
        assert_eq!(stat.shard_id, 0);
        assert_eq!(stat.current_usage, 850);
        assert_eq!(stat.memory_limit, 1000);
        assert_eq!(stat.usage_ratio, 0.85);
        assert!(stat.should_evict);
        assert_eq!(stat.bytes_to_free, 250); // 850 - 600
    }

    #[test]
    fn test_reset() {
        let monitor = MemoryPressureMonitor::new(0, 1000, 0.8, 0.6).unwrap();
        
        monitor.set_usage(500);
        assert_eq!(monitor.current_usage(), 500);
        
        monitor.reset();
        assert_eq!(monitor.current_usage(), 0);
    }
}