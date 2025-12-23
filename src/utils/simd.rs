//! SIMD-accelerated operations for CrabCache
//! 
//! This module provides vectorized operations using SIMD instructions
//! for parsing, key comparison, and hash calculation to maximize performance.

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use crate::protocol::commands::Command;

/// SIMD-accelerated parser for binary protocol
pub struct SIMDParser;

impl SIMDParser {
    /// Parse multiple commands using vectorized operations
    pub fn parse_commands_vectorized(data: &[u8]) -> crate::Result<Vec<Command>> {
        // Check if SIMD is available
        #[cfg(target_arch = "x86_64")]
        {
            if !is_x86_feature_detected!("sse2") {
                return Self::parse_commands_scalar(data);
            }
        }
        
        // For now, implement basic SIMD-accelerated parsing
        // TODO: Implement full vectorized parsing in Phase 3.2
        Self::parse_commands_scalar(data)
    }
    
    /// Compare two keys using SIMD acceleration
    pub fn compare_keys_simd(key1: &[u8], key2: &[u8]) -> bool {
        if key1.len() != key2.len() {
            return false;
        }
        
        // Use SIMD for keys >= 16 bytes on x86_64
        #[cfg(target_arch = "x86_64")]
        {
            if key1.len() >= 16 && is_x86_feature_detected!("sse2") {
                return unsafe { Self::compare_keys_sse2(key1, key2) };
            }
        }
        
        // Fallback to regular comparison
        key1 == key2
    }
    
    /// Calculate hash using SIMD-optimized algorithm
    pub fn hash_key_simd(key: &[u8]) -> u64 {
        #[cfg(target_arch = "x86_64")]
        {
            if key.len() >= 16 && is_x86_feature_detected!("sse2") {
                return unsafe { Self::hash_key_sse2(key) };
            }
        }
        
        Self::hash_key_scalar(key)
    }
    
    /// Validate UTF-8 using SIMD acceleration
    pub fn validate_utf8_simd(data: &[u8]) -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            if data.len() >= 16 && is_x86_feature_detected!("sse2") {
                return unsafe { Self::validate_utf8_sse2(data) };
            }
        }
        
        std::str::from_utf8(data).is_ok()
    }
    
    // Private SIMD implementations (x86_64 only)
    
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sse2")]
    unsafe fn compare_keys_sse2(key1: &[u8], key2: &[u8]) -> bool {
        let len = key1.len();
        let chunks = len / 16;
        let remainder = len % 16;
        
        // Compare 16-byte chunks
        for i in 0..chunks {
            let offset = i * 16;
            let a = _mm_loadu_si128(key1.as_ptr().add(offset) as *const __m128i);
            let b = _mm_loadu_si128(key2.as_ptr().add(offset) as *const __m128i);
            let cmp = _mm_cmpeq_epi8(a, b);
            
            if _mm_movemask_epi8(cmp) != 0xFFFF {
                return false;
            }
        }
        
        // Compare remainder bytes
        if remainder > 0 {
            let offset = chunks * 16;
            for i in 0..remainder {
                if key1[offset + i] != key2[offset + i] {
                    return false;
                }
            }
        }
        
        true
    }
    
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sse2")]
    unsafe fn hash_key_sse2(key: &[u8]) -> u64 {
        let mut hash = 0xcbf29ce484222325u64; // FNV-1a offset basis
        const FNV_PRIME: u64 = 0x100000001b3;
        
        let len = key.len();
        let chunks = len / 16;
        let remainder = len % 16;
        
        // Process 16-byte chunks
        for i in 0..chunks {
            let offset = i * 16;
            let chunk = _mm_loadu_si128(key.as_ptr().add(offset) as *const __m128i);
            
            // Extract bytes and hash them
            let bytes: [u8; 16] = std::mem::transmute(chunk);
            for &byte in &bytes {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
        
        // Process remainder
        if remainder > 0 {
            let offset = chunks * 16;
            for i in 0..remainder {
                hash ^= key[offset + i] as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
        
        hash
    }
    
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sse2")]
    unsafe fn validate_utf8_sse2(data: &[u8]) -> bool {
        // Simplified SIMD UTF-8 validation
        // For now, fall back to standard validation
        // TODO: Implement full SIMD UTF-8 validation
        std::str::from_utf8(data).is_ok()
    }
    
    // Scalar fallback implementations
    
    fn parse_commands_scalar(data: &[u8]) -> crate::Result<Vec<Command>> {
        // For now, parse single command
        // TODO: Implement multi-command parsing
        if data.is_empty() {
            return Ok(Vec::new());
        }
        
        let command = crate::protocol::binary::BinaryProtocol::parse_command(data)?;
        Ok(vec![command])
    }
    
    fn hash_key_scalar(key: &[u8]) -> u64 {
        // FNV-1a hash
        let mut hash = 0xcbf29ce484222325u64;
        const FNV_PRIME: u64 = 0x100000001b3;
        
        for &byte in key {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        
        hash
    }
}

/// SIMD metrics for performance monitoring
#[derive(Debug, Clone, Default)]
pub struct SIMDMetrics {
    pub simd_operations_per_sec: u64,
    pub vectorized_comparisons: u64,
    pub simd_speedup_factor: f64,
    pub fallback_operations: u64,
    pub sse2_available: bool,
    pub avx2_available: bool,
}

impl SIMDMetrics {
    pub fn new() -> Self {
        Self {
            simd_operations_per_sec: 0,
            vectorized_comparisons: 0,
            simd_speedup_factor: 1.0,
            fallback_operations: 0,
            #[cfg(target_arch = "x86_64")]
            sse2_available: is_x86_feature_detected!("sse2"),
            #[cfg(not(target_arch = "x86_64"))]
            sse2_available: false,
            #[cfg(target_arch = "x86_64")]
            avx2_available: is_x86_feature_detected!("avx2"),
            #[cfg(not(target_arch = "x86_64"))]
            avx2_available: false,
        }
    }
    
    pub fn simd_utilization(&self) -> f64 {
        let total_ops = self.vectorized_comparisons + self.fallback_operations;
        if total_ops == 0 {
            0.0
        } else {
            self.vectorized_comparisons as f64 / total_ops as f64 * 100.0
        }
    }
}

/// CPU feature detection and capabilities
pub struct CPUFeatures;

impl CPUFeatures {
    /// Detect available SIMD features
    pub fn detect() -> SIMDCapabilities {
        SIMDCapabilities {
            #[cfg(target_arch = "x86_64")]
            sse2: is_x86_feature_detected!("sse2"),
            #[cfg(not(target_arch = "x86_64"))]
            sse2: false,
            #[cfg(target_arch = "x86_64")]
            sse3: is_x86_feature_detected!("sse3"),
            #[cfg(not(target_arch = "x86_64"))]
            sse3: false,
            #[cfg(target_arch = "x86_64")]
            ssse3: is_x86_feature_detected!("ssse3"),
            #[cfg(not(target_arch = "x86_64"))]
            ssse3: false,
            #[cfg(target_arch = "x86_64")]
            sse4_1: is_x86_feature_detected!("sse4.1"),
            #[cfg(not(target_arch = "x86_64"))]
            sse4_1: false,
            #[cfg(target_arch = "x86_64")]
            sse4_2: is_x86_feature_detected!("sse4.2"),
            #[cfg(not(target_arch = "x86_64"))]
            sse4_2: false,
            #[cfg(target_arch = "x86_64")]
            avx: is_x86_feature_detected!("avx"),
            #[cfg(not(target_arch = "x86_64"))]
            avx: false,
            #[cfg(target_arch = "x86_64")]
            avx2: is_x86_feature_detected!("avx2"),
            #[cfg(not(target_arch = "x86_64"))]
            avx2: false,
            #[cfg(target_arch = "x86_64")]
            avx512f: is_x86_feature_detected!("avx512f"),
            #[cfg(not(target_arch = "x86_64"))]
            avx512f: false,
        }
    }
    
    /// Print CPU capabilities
    pub fn print_capabilities() {
        let caps = Self::detect();
        println!("🔧 CPU SIMD Capabilities:");
        println!("  SSE2: {}", if caps.sse2 { "✅" } else { "❌" });
        println!("  SSE3: {}", if caps.sse3 { "✅" } else { "❌" });
        println!("  SSSE3: {}", if caps.ssse3 { "✅" } else { "❌" });
        println!("  SSE4.1: {}", if caps.sse4_1 { "✅" } else { "❌" });
        println!("  SSE4.2: {}", if caps.sse4_2 { "✅" } else { "❌" });
        println!("  AVX: {}", if caps.avx { "✅" } else { "❌" });
        println!("  AVX2: {}", if caps.avx2 { "✅" } else { "❌" });
        println!("  AVX-512F: {}", if caps.avx512f { "✅" } else { "❌" });
        
        let recommended = if caps.avx2 {
            "AVX2 (Excellent performance)"
        } else if caps.sse4_2 {
            "SSE4.2 (Good performance)"
        } else if caps.sse2 {
            "SSE2 (Basic acceleration)"
        } else {
            "No SIMD (Scalar fallback)"
        };
        
        println!("  Recommended: {}", recommended);
    }
}

/// SIMD capabilities structure
#[derive(Debug, Clone)]
pub struct SIMDCapabilities {
    pub sse2: bool,
    pub sse3: bool,
    pub ssse3: bool,
    pub sse4_1: bool,
    pub sse4_2: bool,
    pub avx: bool,
    pub avx2: bool,
    pub avx512f: bool,
}

impl SIMDCapabilities {
    /// Get the best available instruction set
    pub fn best_instruction_set(&self) -> &'static str {
        if self.avx512f {
            "AVX-512F"
        } else if self.avx2 {
            "AVX2"
        } else if self.avx {
            "AVX"
        } else if self.sse4_2 {
            "SSE4.2"
        } else if self.sse4_1 {
            "SSE4.1"
        } else if self.ssse3 {
            "SSSE3"
        } else if self.sse3 {
            "SSE3"
        } else if self.sse2 {
            "SSE2"
        } else {
            "Scalar"
        }
    }
    
    /// Estimate performance multiplier
    pub fn performance_multiplier(&self) -> f64 {
        if self.avx512f {
            8.0 // 512-bit vectors
        } else if self.avx2 {
            4.0 // 256-bit vectors
        } else if self.avx {
            3.0 // 256-bit vectors (limited)
        } else if self.sse4_2 {
            2.5 // 128-bit vectors + advanced ops
        } else if self.sse2 {
            2.0 // 128-bit vectors
        } else {
            1.0 // Scalar
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cpu_feature_detection() {
        let caps = CPUFeatures::detect();
        
        // At least SSE2 should be available on x86_64
        #[cfg(target_arch = "x86_64")]
        assert!(caps.sse2);
        
        println!("Detected capabilities: {:?}", caps);
        println!("Best instruction set: {}", caps.best_instruction_set());
        println!("Performance multiplier: {:.1}x", caps.performance_multiplier());
    }
    
    #[test]
    fn test_key_comparison() {
        let key1 = b"test_key_1234567890";
        let key2 = b"test_key_1234567890";
        let key3 = b"different_key_12345";
        
        assert!(SIMDParser::compare_keys_simd(key1, key2));
        assert!(!SIMDParser::compare_keys_simd(key1, key3));
        
        // Test short keys
        let short1 = b"abc";
        let short2 = b"abc";
        let short3 = b"def";
        
        assert!(SIMDParser::compare_keys_simd(short1, short2));
        assert!(!SIMDParser::compare_keys_simd(short1, short3));
    }
    
    #[test]
    fn test_hash_consistency() {
        let key = b"test_key_for_hashing";
        
        let hash1 = SIMDParser::hash_key_simd(key);
        let hash2 = SIMDParser::hash_key_simd(key);
        
        assert_eq!(hash1, hash2);
        
        // Different keys should have different hashes (with high probability)
        let different_key = b"different_key_hash";
        let hash3 = SIMDParser::hash_key_simd(different_key);
        
        assert_ne!(hash1, hash3);
    }
    
    #[test]
    fn test_simd_metrics() {
        let mut metrics = SIMDMetrics::new();
        
        metrics.vectorized_comparisons = 80;
        metrics.fallback_operations = 20;
        
        assert_eq!(metrics.simd_utilization(), 80.0);
        
        println!("SIMD utilization: {:.1}%", metrics.simd_utilization());
        println!("SSE2 available: {}", metrics.sse2_available);
        println!("AVX2 available: {}", metrics.avx2_available);
    }
}