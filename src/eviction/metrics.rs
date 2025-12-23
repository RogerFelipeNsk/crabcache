//! Eviction metrics collection and reporting

use std::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};

/// Thread-safe eviction metrics
#[derive(Debug, Default)]
pub struct EvictionMetrics {
    /// Total number of cache requests
    total_requests: AtomicU64,
    /// Number of cache hits
    cache_hits: AtomicU64,
    /// Number of cache misses
    cache_misses: AtomicU64,
    /// Number of items evicted
    evictions: AtomicU64,
    /// Number of promotions from Window to Main LRU
    window_promotions: AtomicU64,
    /// Number of admission requests accepted
    admissions_accepted: AtomicU64,
    /// Number of admission requests rejected
    admissions_rejected: AtomicU64,
    /// Number of frequency sketch resets
    sketch_resets: AtomicU64,
    /// Number of items currently in Window LRU
    window_size: AtomicU64,
    /// Number of items currently in Main LRU
    main_size: AtomicU64,
}

/// Snapshot of eviction metrics for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvictionMetricsSnapshot {
    pub total_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub evictions: u64,
    pub window_promotions: u64,
    pub admissions_accepted: u64,
    pub admissions_rejected: u64,
    pub sketch_resets: u64,
    pub window_size: u64,
    pub main_size: u64,
    pub hit_ratio: f64,
    pub admission_ratio: f64,
}

impl EvictionMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Record a cache hit
    pub fn record_hit(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record a cache miss
    pub fn record_miss(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record an eviction
    pub fn record_eviction(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record multiple evictions
    pub fn record_evictions(&self, count: usize) {
        self.evictions.fetch_add(count as u64, Ordering::Relaxed);
    }
    
    /// Record a promotion from Window to Main LRU
    pub fn record_promotion(&self) {
        self.window_promotions.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record an accepted admission
    pub fn record_admission_accepted(&self) {
        self.admissions_accepted.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record a rejected admission
    pub fn record_admission_rejected(&self) {
        self.admissions_rejected.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record a frequency sketch reset
    pub fn record_sketch_reset(&self) {
        self.sketch_resets.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Update window size
    pub fn set_window_size(&self, size: usize) {
        self.window_size.store(size as u64, Ordering::Relaxed);
    }
    
    /// Update main size
    pub fn set_main_size(&self, size: usize) {
        self.main_size.store(size as u64, Ordering::Relaxed);
    }
    
    /// Get total requests
    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }
    
    /// Get cache hits
    pub fn cache_hits(&self) -> u64 {
        self.cache_hits.load(Ordering::Relaxed)
    }
    
    /// Get cache misses
    pub fn cache_misses(&self) -> u64 {
        self.cache_misses.load(Ordering::Relaxed)
    }
    
    /// Get evictions
    pub fn evictions(&self) -> u64 {
        self.evictions.load(Ordering::Relaxed)
    }
    
    /// Calculate hit ratio
    pub fn hit_ratio(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            let hits = self.cache_hits.load(Ordering::Relaxed);
            hits as f64 / total as f64
        }
    }
    
    /// Calculate admission ratio
    pub fn admission_ratio(&self) -> f64 {
        let accepted = self.admissions_accepted.load(Ordering::Relaxed);
        let rejected = self.admissions_rejected.load(Ordering::Relaxed);
        let total = accepted + rejected;
        
        if total == 0 {
            0.0
        } else {
            accepted as f64 / total as f64
        }
    }
    
    /// Get a snapshot of all metrics
    pub fn snapshot(&self) -> EvictionMetricsSnapshot {
        EvictionMetricsSnapshot {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            window_promotions: self.window_promotions.load(Ordering::Relaxed),
            admissions_accepted: self.admissions_accepted.load(Ordering::Relaxed),
            admissions_rejected: self.admissions_rejected.load(Ordering::Relaxed),
            sketch_resets: self.sketch_resets.load(Ordering::Relaxed),
            window_size: self.window_size.load(Ordering::Relaxed),
            main_size: self.main_size.load(Ordering::Relaxed),
            hit_ratio: self.hit_ratio(),
            admission_ratio: self.admission_ratio(),
        }
    }
    
    /// Reset all metrics to zero
    pub fn reset(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.evictions.store(0, Ordering::Relaxed);
        self.window_promotions.store(0, Ordering::Relaxed);
        self.admissions_accepted.store(0, Ordering::Relaxed);
        self.admissions_rejected.store(0, Ordering::Relaxed);
        self.sketch_resets.store(0, Ordering::Relaxed);
        self.window_size.store(0, Ordering::Relaxed);
        self.main_size.store(0, Ordering::Relaxed);
    }
}

impl Clone for EvictionMetrics {
    fn clone(&self) -> Self {
        let snapshot = self.snapshot();
        Self {
            total_requests: AtomicU64::new(snapshot.total_requests),
            cache_hits: AtomicU64::new(snapshot.cache_hits),
            cache_misses: AtomicU64::new(snapshot.cache_misses),
            evictions: AtomicU64::new(snapshot.evictions),
            window_promotions: AtomicU64::new(snapshot.window_promotions),
            admissions_accepted: AtomicU64::new(snapshot.admissions_accepted),
            admissions_rejected: AtomicU64::new(snapshot.admissions_rejected),
            sketch_resets: AtomicU64::new(snapshot.sketch_resets),
            window_size: AtomicU64::new(snapshot.window_size),
            main_size: AtomicU64::new(snapshot.main_size),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_metrics() {
        let metrics = EvictionMetrics::new();
        assert_eq!(metrics.total_requests(), 0);
        assert_eq!(metrics.cache_hits(), 0);
        assert_eq!(metrics.cache_misses(), 0);
        assert_eq!(metrics.hit_ratio(), 0.0);
        assert_eq!(metrics.admission_ratio(), 0.0);
    }

    #[test]
    fn test_record_hit() {
        let metrics = EvictionMetrics::new();
        
        metrics.record_hit();
        assert_eq!(metrics.total_requests(), 1);
        assert_eq!(metrics.cache_hits(), 1);
        assert_eq!(metrics.cache_misses(), 0);
        assert_eq!(metrics.hit_ratio(), 1.0);
    }

    #[test]
    fn test_record_miss() {
        let metrics = EvictionMetrics::new();
        
        metrics.record_miss();
        assert_eq!(metrics.total_requests(), 1);
        assert_eq!(metrics.cache_hits(), 0);
        assert_eq!(metrics.cache_misses(), 1);
        assert_eq!(metrics.hit_ratio(), 0.0);
    }

    #[test]
    fn test_hit_ratio_calculation() {
        let metrics = EvictionMetrics::new();
        
        // 3 hits, 2 misses = 60% hit ratio
        metrics.record_hit();
        metrics.record_hit();
        metrics.record_hit();
        metrics.record_miss();
        metrics.record_miss();
        
        assert_eq!(metrics.total_requests(), 5);
        assert_eq!(metrics.cache_hits(), 3);
        assert_eq!(metrics.cache_misses(), 2);
        assert!((metrics.hit_ratio() - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_admission_ratio() {
        let metrics = EvictionMetrics::new();
        
        // 2 accepted, 3 rejected = 40% admission ratio
        metrics.record_admission_accepted();
        metrics.record_admission_accepted();
        metrics.record_admission_rejected();
        metrics.record_admission_rejected();
        metrics.record_admission_rejected();
        
        assert!((metrics.admission_ratio() - 0.4).abs() < f64::EPSILON);
    }

    #[test]
    fn test_eviction_recording() {
        let metrics = EvictionMetrics::new();
        
        metrics.record_eviction();
        metrics.record_evictions(5);
        
        assert_eq!(metrics.evictions(), 6);
    }

    #[test]
    fn test_size_updates() {
        let metrics = EvictionMetrics::new();
        
        metrics.set_window_size(10);
        metrics.set_main_size(100);
        
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.window_size, 10);
        assert_eq!(snapshot.main_size, 100);
    }

    #[test]
    fn test_snapshot() {
        let metrics = EvictionMetrics::new();
        
        metrics.record_hit();
        metrics.record_miss();
        metrics.record_eviction();
        metrics.record_promotion();
        metrics.record_admission_accepted();
        metrics.record_sketch_reset();
        metrics.set_window_size(5);
        metrics.set_main_size(50);
        
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_requests, 2);
        assert_eq!(snapshot.cache_hits, 1);
        assert_eq!(snapshot.cache_misses, 1);
        assert_eq!(snapshot.evictions, 1);
        assert_eq!(snapshot.window_promotions, 1);
        assert_eq!(snapshot.admissions_accepted, 1);
        assert_eq!(snapshot.sketch_resets, 1);
        assert_eq!(snapshot.window_size, 5);
        assert_eq!(snapshot.main_size, 50);
        assert_eq!(snapshot.hit_ratio, 0.5);
    }

    #[test]
    fn test_reset() {
        let metrics = EvictionMetrics::new();
        
        metrics.record_hit();
        metrics.record_eviction();
        metrics.set_window_size(10);
        
        assert_ne!(metrics.total_requests(), 0);
        
        metrics.reset();
        
        assert_eq!(metrics.total_requests(), 0);
        assert_eq!(metrics.cache_hits(), 0);
        assert_eq!(metrics.evictions(), 0);
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.window_size, 0);
    }

    #[test]
    fn test_clone() {
        let metrics = EvictionMetrics::new();
        
        metrics.record_hit();
        metrics.record_eviction();
        
        let cloned = metrics.clone();
        
        assert_eq!(cloned.total_requests(), metrics.total_requests());
        assert_eq!(cloned.cache_hits(), metrics.cache_hits());
        assert_eq!(cloned.evictions(), metrics.evictions());
    }
}