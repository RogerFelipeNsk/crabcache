//! Rate limiting module for CrabCache

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    capacity: f64,
    refill_rate: f64, // tokens per second
}

impl TokenBucket {
    fn new(capacity: u32, refill_rate: u32) -> Self {
        Self {
            tokens: capacity as f64,
            last_refill: Instant::now(),
            capacity: capacity as f64,
            refill_rate: refill_rate as f64,
        }
    }
    
    fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();
        
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }
    
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        
        if elapsed > 0.0 {
            let tokens_to_add = elapsed * self.refill_rate;
            self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
            self.last_refill = now;
        }
    }
}

/// Rate limiter using token bucket algorithm
pub struct RateLimiter {
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    max_requests_per_second: u32,
    burst_capacity: u32,
    cleanup_interval: Duration,
    last_cleanup: Arc<RwLock<Instant>>,
}

impl RateLimiter {
    /// Create new rate limiter
    pub fn new(max_requests_per_second: u32, burst_capacity: u32) -> Self {
        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            max_requests_per_second,
            burst_capacity,
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            last_cleanup: Arc::new(RwLock::new(Instant::now())),
        }
    }
    
    /// Check if request is allowed for the given key (usually client IP)
    pub async fn check_rate(&self, key: &str) -> RateLimitResult {
        // Periodic cleanup of old buckets
        self.cleanup_if_needed().await;
        
        let mut buckets = self.buckets.write().await;
        
        let bucket = buckets.entry(key.to_string()).or_insert_with(|| {
            debug!("Creating new rate limit bucket for key: {}", key);
            TokenBucket::new(self.burst_capacity, self.max_requests_per_second)
        });
        
        if bucket.try_consume(1.0) {
            RateLimitResult::Allowed
        } else {
            warn!("Rate limit exceeded for key: {}", key);
            RateLimitResult::RateLimited
        }
    }
    
    /// Get current token count for a key (for monitoring)
    pub async fn get_tokens(&self, key: &str) -> Option<f64> {
        let mut buckets = self.buckets.write().await;
        if let Some(bucket) = buckets.get_mut(key) {
            bucket.refill();
            Some(bucket.tokens)
        } else {
            None
        }
    }
    
    /// Get number of active buckets
    pub async fn active_buckets(&self) -> usize {
        let buckets = self.buckets.read().await;
        buckets.len()
    }
    
    /// Cleanup old buckets that haven't been used recently
    async fn cleanup_if_needed(&self) {
        let mut last_cleanup = self.last_cleanup.write().await;
        let now = Instant::now();
        
        if now.duration_since(*last_cleanup) > self.cleanup_interval {
            *last_cleanup = now;
            drop(last_cleanup); // Release the lock
            
            self.cleanup_old_buckets().await;
        }
    }
    
    async fn cleanup_old_buckets(&self) {
        let mut buckets = self.buckets.write().await;
        let now = Instant::now();
        let cleanup_threshold = Duration::from_secs(600); // 10 minutes
        
        let initial_count = buckets.len();
        buckets.retain(|_key, bucket| {
            now.duration_since(bucket.last_refill) < cleanup_threshold
        });
        
        let removed = initial_count - buckets.len();
        if removed > 0 {
            debug!("Cleaned up {} old rate limit buckets", removed);
        }
    }
    
    /// Reset rate limit for a specific key
    pub async fn reset_key(&self, key: &str) -> bool {
        let mut buckets = self.buckets.write().await;
        if buckets.remove(key).is_some() {
            debug!("Reset rate limit for key: {}", key);
            true
        } else {
            false
        }
    }
    
    /// Clear all rate limit buckets
    pub async fn clear_all(&self) {
        let mut buckets = self.buckets.write().await;
        let count = buckets.len();
        buckets.clear();
        debug!("Cleared all {} rate limit buckets", count);
    }
}

/// Rate limiting result
#[derive(Debug, Clone, PartialEq)]
pub enum RateLimitResult {
    /// Request is allowed
    Allowed,
    /// Request is rate limited
    RateLimited,
}

impl RateLimitResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, RateLimitResult::Allowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(10, 5); // 10 capacity, 5 tokens/sec
        
        // Should allow initial burst
        for _ in 0..10 {
            assert!(bucket.try_consume(1.0));
        }
        
        // Should deny when empty
        assert!(!bucket.try_consume(1.0));
    }
    
    #[tokio::test]
    async fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(5, 10); // 5 capacity, 10 tokens/sec
        
        // Consume all tokens
        for _ in 0..5 {
            assert!(bucket.try_consume(1.0));
        }
        assert!(!bucket.try_consume(1.0));
        
        // Wait for refill (simulate time passing)
        sleep(Duration::from_millis(600)).await; // 0.6 seconds = 6 tokens
        
        // Should have refilled to capacity (5 tokens max)
        for _ in 0..5 {
            assert!(bucket.try_consume(1.0));
        }
        assert!(!bucket.try_consume(1.0));
    }
    
    #[tokio::test]
    async fn test_rate_limiter() {
        let rate_limiter = RateLimiter::new(5, 10); // 5 req/sec, 10 burst
        
        // Should allow initial burst
        for i in 0..10 {
            let result = rate_limiter.check_rate("test_client").await;
            assert_eq!(result, RateLimitResult::Allowed, "Request {} should be allowed", i);
        }
        
        // Should deny next request
        let result = rate_limiter.check_rate("test_client").await;
        assert_eq!(result, RateLimitResult::RateLimited);
    }
    
    #[tokio::test]
    async fn test_rate_limiter_different_keys() {
        let rate_limiter = RateLimiter::new(2, 2); // 2 req/sec, 2 burst
        
        // Each key should have its own bucket
        assert_eq!(rate_limiter.check_rate("client1").await, RateLimitResult::Allowed);
        assert_eq!(rate_limiter.check_rate("client2").await, RateLimitResult::Allowed);
        assert_eq!(rate_limiter.check_rate("client1").await, RateLimitResult::Allowed);
        assert_eq!(rate_limiter.check_rate("client2").await, RateLimitResult::Allowed);
        
        // Both should be rate limited now
        assert_eq!(rate_limiter.check_rate("client1").await, RateLimitResult::RateLimited);
        assert_eq!(rate_limiter.check_rate("client2").await, RateLimitResult::RateLimited);
    }
}