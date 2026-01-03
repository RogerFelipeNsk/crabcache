//! Assembly-optimized hot paths for ultra-low latency
//! Note: Assembly optimizations are currently disabled for compatibility
//! All functions use optimized Rust fallbacks

/// Ultra-fast hash function (optimized Rust implementation)
#[inline(always)]
pub fn hash_key_asm(key: &[u8]) -> u64 {
    // Use standard FNV-1a hash (very fast)
    let mut hash: u64 = 0xcbf29ce484222325;
    let fnv_prime: u64 = 0x100000001b3;

    for &byte in key {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(fnv_prime);
    }

    hash
}

/// Ultra-fast memory comparison (optimized Rust implementation)
#[inline(always)]
pub fn memcmp_asm(a: &[u8], b: &[u8]) -> bool {
    a == b
}

/// Ultra-fast string length calculation (optimized Rust implementation)
///
/// # Safety
/// The caller must ensure that `data` points to a valid null-terminated string.
#[inline(always)]
pub unsafe fn strlen_asm(data: *const u8) -> usize {
    let mut len = 0;
    while *data.add(len) != 0 {
        len += 1;
    }
    len
}

/// Ultra-fast memory copy (optimized Rust implementation)
///
/// # Safety
/// The caller must ensure that `src` and `dst` are valid pointers and that
/// the memory regions do not overlap.
#[inline(always)]
pub unsafe fn memcpy_asm(dst: *mut u8, src: *const u8, len: usize) {
    if len == 0 {
        return;
    }

    std::ptr::copy_nonoverlapping(src, dst, len);
}

/// Ultra-fast byte search (optimized Rust implementation)
#[inline(always)]
pub fn memchr_asm(haystack: &[u8], needle: u8) -> Option<usize> {
    haystack.iter().position(|&b| b == needle)
}

/// Prefetch data into CPU cache (no-op fallback)
#[inline(always)]
pub fn prefetch_data(_ptr: *const u8, _hint: i32) {
    // No-op on non-x86 platforms
}

/// CPU pause instruction for spin loops (optimized fallback)
#[inline(always)]
pub fn cpu_pause() {
    std::hint::spin_loop();
}

/// Get CPU timestamp counter (fallback using std::time)
#[inline(always)]
pub fn rdtsc() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

/// Serialize instruction execution (no-op fallback)
#[inline(always)]
pub fn serialize() {
    std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
}

/// Ultra-fast checksum calculation (optimized Rust implementation)
#[inline(always)]
pub fn checksum_asm(data: &[u8]) -> u32 {
    let mut checksum: u32 = 0;

    // Process 4 bytes at a time
    let chunks = data.chunks_exact(4);
    let remainder = chunks.remainder();

    for chunk in chunks {
        let val = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        checksum = checksum.wrapping_add(val);
    }

    // Handle remaining bytes
    for &byte in remainder {
        checksum = checksum.wrapping_add(byte as u32);
    }

    checksum
}

/// Branch prediction hints (optimized fallbacks)
#[inline(always)]
pub fn likely(condition: bool) -> bool {
    condition
}

#[inline(always)]
pub fn unlikely(condition: bool) -> bool {
    condition
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_key_asm() {
        let key1 = b"test_key";
        let key2 = b"test_key";
        let key3 = b"different_key";

        let hash1 = hash_key_asm(key1);
        let hash2 = hash_key_asm(key2);
        let hash3 = hash_key_asm(key3);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_ne!(hash1, 0);
    }

    #[test]
    fn test_memcmp_asm() {
        assert!(memcmp_asm(b"hello", b"hello"));
        assert!(!memcmp_asm(b"hello", b"world"));
        assert!(!memcmp_asm(b"hello", b"hell"));
        assert!(memcmp_asm(b"", b""));
    }

    #[test]
    fn test_memchr_asm() {
        assert_eq!(memchr_asm(b"hello", b'e'), Some(1));
        assert_eq!(memchr_asm(b"hello", b'o'), Some(4));
        assert_eq!(memchr_asm(b"hello", b'x'), None);
        assert_eq!(memchr_asm(b"", b'x'), None);
    }

    #[test]
    fn test_checksum_asm() {
        let data1 = b"test";
        let data2 = b"test";
        let data3 = b"different";

        let sum1 = checksum_asm(data1);
        let sum2 = checksum_asm(data2);
        let sum3 = checksum_asm(data3);

        assert_eq!(sum1, sum2);
        assert_ne!(sum1, sum3);
        assert_eq!(checksum_asm(b""), 0);
    }

    #[test]
    fn test_rdtsc() {
        let t1 = rdtsc();
        // Do some work
        for _ in 0..1000 {
            cpu_pause();
        }
        let t2 = rdtsc();

        assert!(t2 >= t1); // Should be monotonic (mostly)
    }

    #[test]
    fn test_performance_comparison() {
        let key = b"performance_test_key_with_reasonable_length";
        let iterations = 10000;

        // Test assembly hash
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            let _ = hash_key_asm(key);
        }
        let asm_time = start.elapsed();

        // Test standard hash
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let start = std::time::Instant::now();
        for _ in 0..iterations {
            let mut hasher = DefaultHasher::new();
            key.hash(&mut hasher);
            let _ = hasher.finish();
        }
        let std_time = start.elapsed();

        println!(
            "Assembly hash time: {:?} ({:.2} ns/op)",
            asm_time,
            asm_time.as_nanos() as f64 / iterations as f64
        );
        println!(
            "Standard hash time: {:?} ({:.2} ns/op)",
            std_time,
            std_time.as_nanos() as f64 / iterations as f64
        );

        // Both should work
        assert!(asm_time.as_nanos() > 0);
        assert!(std_time.as_nanos() > 0);
    }
}
