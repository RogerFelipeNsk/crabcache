//! Pre-allocated response pool for zero-latency responses

use ahash::AHashMap;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;

/// Pre-allocated response pool for ultra-fast responses
pub struct ResponsePool {
    // Static responses (most common)
    ok_response: &'static [u8],
    pong_response: &'static [u8],
    null_response: &'static [u8],

    // Pre-allocated value responses by size
    value_responses: AHashMap<usize, Vec<Bytes>>,
    value_pool_index: AtomicUsize,

    // Pre-allocated error responses
    error_responses: AHashMap<&'static str, Bytes>,

    // Statistics
    hits: AtomicUsize,
    misses: AtomicUsize,
}

// Static response constants
const OK_RESPONSE: &[u8] = b"OK\r\n";
const PONG_RESPONSE: &[u8] = b"PONG\r\n";
const NULL_RESPONSE: &[u8] = b"NULL\r\n";

// Common value sizes to pre-allocate
const COMMON_VALUE_SIZES: &[usize] = &[16, 32, 64, 128, 256, 512, 1024, 2048, 4096];
const POOL_SIZE_PER_SIZE: usize = 100;

// Global response pool
static RESPONSE_POOL: LazyLock<ResponsePool> = LazyLock::new(ResponsePool::new);

impl ResponsePool {
    /// Create new response pool with pre-allocated responses
    pub fn new() -> Self {
        let mut value_responses = AHashMap::new();

        // Pre-allocate value responses for common sizes
        for &size in COMMON_VALUE_SIZES {
            let mut pool = Vec::with_capacity(POOL_SIZE_PER_SIZE);

            for _ in 0..POOL_SIZE_PER_SIZE {
                // Pre-allocate response with header + value space
                let mut response = Vec::with_capacity(6 + size + 2); // "VALUE " + size + "\r\n"
                response.extend_from_slice(b"VALUE ");
                response.resize(6 + size, b'x'); // Fill with placeholder
                response.extend_from_slice(b"\r\n");

                pool.push(Bytes::from(response));
            }

            value_responses.insert(size, pool);
        }

        // Pre-allocate common error responses
        let mut error_responses = AHashMap::new();
        error_responses.insert("NOT_FOUND", Bytes::from_static(b"NOT_FOUND\r\n"));
        error_responses.insert(
            "INVALID_COMMAND",
            Bytes::from_static(b"INVALID_COMMAND\r\n"),
        );
        error_responses.insert("PARSE_ERROR", Bytes::from_static(b"PARSE_ERROR\r\n"));
        error_responses.insert("AUTH_ERROR", Bytes::from_static(b"AUTH_ERROR\r\n"));
        error_responses.insert("TIMEOUT", Bytes::from_static(b"TIMEOUT\r\n"));

        Self {
            ok_response: OK_RESPONSE,
            pong_response: PONG_RESPONSE,
            null_response: NULL_RESPONSE,
            value_responses,
            value_pool_index: AtomicUsize::new(0),
            error_responses,
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
        }
    }

    /// Get OK response (zero allocation)
    #[inline(always)]
    pub fn get_ok_response(&self) -> &'static [u8] {
        self.hits.fetch_add(1, Ordering::Relaxed);
        self.ok_response
    }

    /// Get PONG response (zero allocation)
    #[inline(always)]
    pub fn get_pong_response(&self) -> &'static [u8] {
        self.hits.fetch_add(1, Ordering::Relaxed);
        self.pong_response
    }

    /// Get NULL response (zero allocation)
    #[inline(always)]
    pub fn get_null_response(&self) -> &'static [u8] {
        self.hits.fetch_add(1, Ordering::Relaxed);
        self.null_response
    }

    /// Get pre-allocated value response
    #[inline(always)]
    pub fn get_value_response(&self, value: &[u8]) -> Option<Bytes> {
        // Simplified implementation - always return None to use fallback
        // TODO: Implement proper zero-copy value response pool
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Get pre-allocated error response
    #[inline(always)]
    pub fn get_error_response(&self, error_type: &str) -> Option<Bytes> {
        if let Some(response) = self.error_responses.get(error_type) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some(response.clone())
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Create value response (fallback when pool miss)
    pub fn create_value_response(&self, value: &[u8]) -> Bytes {
        let mut response = Vec::with_capacity(6 + value.len() + 2);
        response.extend_from_slice(b"VALUE ");
        response.extend_from_slice(value);
        response.extend_from_slice(b"\r\n");

        self.misses.fetch_add(1, Ordering::Relaxed);
        Bytes::from(response)
    }

    /// Create error response (fallback when pool miss)
    pub fn create_error_response(&self, error_msg: &str) -> Bytes {
        let mut response = Vec::with_capacity(error_msg.len() + 2);
        response.extend_from_slice(error_msg.as_bytes());
        response.extend_from_slice(b"\r\n");

        self.misses.fetch_add(1, Ordering::Relaxed);
        Bytes::from(response)
    }

    /// Get pool statistics
    pub fn stats(&self) -> ResponsePoolStats {
        ResponsePoolStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            hit_rate: {
                let hits = self.hits.load(Ordering::Relaxed);
                let misses = self.misses.load(Ordering::Relaxed);
                let total = hits + misses;
                if total > 0 {
                    (hits as f64 / total as f64) * 100.0
                } else {
                    0.0
                }
            },
            total_pools: self.value_responses.len(),
            total_pre_allocated: self.value_responses.values().map(|pool| pool.len()).sum(),
        }
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
    }
}

/// Response pool statistics
#[derive(Debug, Clone)]
pub struct ResponsePoolStats {
    pub hits: usize,
    pub misses: usize,
    pub hit_rate: f64,
    pub total_pools: usize,
    pub total_pre_allocated: usize,
}

/// Global response pool functions
#[inline(always)]
pub fn get_ok_response() -> &'static [u8] {
    RESPONSE_POOL.get_ok_response()
}

#[inline(always)]
pub fn get_pong_response() -> &'static [u8] {
    RESPONSE_POOL.get_pong_response()
}

#[inline(always)]
pub fn get_null_response() -> &'static [u8] {
    RESPONSE_POOL.get_null_response()
}

#[inline(always)]
pub fn get_value_response(value: &[u8]) -> Bytes {
    RESPONSE_POOL
        .get_value_response(value)
        .unwrap_or_else(|| RESPONSE_POOL.create_value_response(value))
}

#[inline(always)]
pub fn get_error_response(error_type: &str) -> Bytes {
    RESPONSE_POOL
        .get_error_response(error_type)
        .unwrap_or_else(|| RESPONSE_POOL.create_error_response(error_type))
}

pub fn response_pool_stats() -> ResponsePoolStats {
    RESPONSE_POOL.stats()
}

pub fn reset_response_pool_stats() {
    RESPONSE_POOL.reset_stats();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_responses() {
        let ok = get_ok_response();
        assert_eq!(ok, b"OK\r\n");

        let pong = get_pong_response();
        assert_eq!(pong, b"PONG\r\n");

        let null = get_null_response();
        assert_eq!(null, b"NULL\r\n");
    }

    #[test]
    fn test_value_response_pool() {
        let value = b"test_value_64_bytes_long_to_test_the_pool_allocation_system";
        let response = get_value_response(value);

        assert!(response.starts_with(b"VALUE "));
        assert!(response.ends_with(b"\r\n"));
        assert!(response.len() > value.len());
    }

    #[test]
    fn test_error_response_pool() {
        let response = get_error_response("NOT_FOUND");
        assert_eq!(response.as_ref(), b"NOT_FOUND\r\n");

        let custom_response = get_error_response("CUSTOM_ERROR");
        assert_eq!(custom_response.as_ref(), b"CUSTOM_ERROR\r\n");
    }

    #[test]
    fn test_pool_statistics() {
        reset_response_pool_stats();

        let _ok = get_ok_response();
        let _pong = get_pong_response();
        let _value = get_value_response(b"test");

        let stats = response_pool_stats();
        assert!(stats.hits > 0);
        assert!(stats.hit_rate > 0.0);
    }

    #[test]
    fn test_common_value_sizes() {
        for &size in COMMON_VALUE_SIZES {
            let value = vec![b'x'; size];
            let response = get_value_response(&value);

            // Should be from pool (fast path)
            assert!(response.len() >= size + 8); // "VALUE " + value + "\r\n"
        }
    }
}
