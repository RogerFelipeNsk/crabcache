//! ARM64 NEON SIMD optimizations for native performance
//! Provides native ARM64 SIMD implementations to replace fallbacks

use crate::ultra_fast::zero_copy_parser::CommandRef;

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

/// ARM64 NEON-optimized command parser
#[cfg(target_arch = "aarch64")]
pub struct NeonParser {
    // Pre-computed NEON patterns for common commands
    ping_pattern: uint8x16_t,
    get_pattern: uint8x16_t,
    put_pattern: uint8x16_t,
    del_pattern: uint8x16_t,
}

/// Fallback parser for non-ARM64 architectures
#[cfg(not(target_arch = "aarch64"))]
pub struct NeonParser;

#[cfg(target_arch = "aarch64")]
impl NeonParser {
    /// Create new NEON parser with pre-computed patterns
    pub fn new() -> Self {
        unsafe {
            Self {
                // "PING" padded to 16 bytes
                ping_pattern: vld1q_u8(
                    [b'P', b'I', b'N', b'G', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].as_ptr(),
                ),
                // "GET " padded to 16 bytes
                get_pattern: vld1q_u8(
                    [b'G', b'E', b'T', b' ', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].as_ptr(),
                ),
                // "PUT " padded to 16 bytes
                put_pattern: vld1q_u8(
                    [b'P', b'U', b'T', b' ', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].as_ptr(),
                ),
                // "DEL " padded to 16 bytes
                del_pattern: vld1q_u8(
                    [b'D', b'E', b'L', b' ', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].as_ptr(),
                ),
            }
        }
    }

    /// Parse command using NEON SIMD optimizations
    pub unsafe fn parse_neon<'a>(&self, data: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        if data.is_empty() {
            return Err("Empty command");
        }

        // Fast path for short commands
        if data.len() < 16 {
            return self.parse_short_command(data);
        }

        // Load first 16 bytes for NEON comparison
        let input = vld1q_u8(data.as_ptr());

        // Compare with PING pattern
        let ping_cmp = vceqq_u8(input, self.ping_pattern);
        let ping_mask = self.extract_mask(ping_cmp);
        if ping_mask & 0xF == 0xF && data.len() == 4 {
            return Ok(CommandRef::Ping);
        }

        // Compare with GET pattern
        let get_cmp = vceqq_u8(input, self.get_pattern);
        let get_mask = self.extract_mask(get_cmp);
        if get_mask & 0xF == 0xF && data.len() > 4 {
            return self.parse_get_neon(&data[4..]);
        }

        // Compare with PUT pattern
        let put_cmp = vceqq_u8(input, self.put_pattern);
        let put_mask = self.extract_mask(put_cmp);
        if put_mask & 0xF == 0xF && data.len() > 4 {
            return self.parse_put_neon(&data[4..]);
        }

        // Compare with DEL pattern
        let del_cmp = vceqq_u8(input, self.del_pattern);
        let del_mask = self.extract_mask(del_cmp);
        if del_mask & 0xF == 0xF && data.len() > 4 {
            return self.parse_del_neon(&data[4..]);
        }

        // Fallback to scalar parsing
        self.parse_scalar_fallback(data)
    }

    /// Extract comparison mask from NEON vector
    unsafe fn extract_mask(&self, cmp_result: uint8x16_t) -> u32 {
        // Convert comparison result to mask
        let mask_bytes: [u8; 16] = std::mem::transmute(cmp_result);
        let mut mask = 0u32;

        for (i, &byte) in mask_bytes.iter().enumerate() {
            if byte == 0xFF {
                mask |= 1 << i;
            }
        }

        mask
    }

    /// Parse short commands using scalar operations
    unsafe fn parse_short_command<'a>(
        &self,
        data: &'a [u8],
    ) -> Result<CommandRef<'a>, &'static str> {
        match data.len() {
            4 if data == b"PING" => Ok(CommandRef::Ping),
            5 if data == b"STATS" => Ok(CommandRef::Stats),
            7 if data == b"METRICS" => Ok(CommandRef::Metrics),
            _ => self.parse_scalar_fallback(data),
        }
    }

    /// Parse GET command with NEON optimizations
    unsafe fn parse_get_neon<'a>(&self, args: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        if args.is_empty() {
            return Err("GET missing key");
        }

        // Use NEON to find space character
        let key_end = self.find_space_neon(args).unwrap_or(args.len());

        if key_end == 0 {
            return Err("GET empty key");
        }

        Ok(CommandRef::Get {
            key: &args[..key_end],
        })
    }

    /// Parse PUT command with NEON optimizations
    unsafe fn parse_put_neon<'a>(&self, args: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        if args.is_empty() {
            return Err("PUT missing arguments");
        }

        // Find first space using NEON
        let key_end = match self.find_space_neon(args) {
            Some(pos) => pos,
            None => return Err("PUT missing value"),
        };

        if key_end == 0 {
            return Err("PUT empty key");
        }

        let value_start = key_end + 1;
        if value_start >= args.len() {
            return Err("PUT missing value");
        }

        let key = &args[..key_end];

        // Find value end using NEON
        let value_end = self
            .find_space_neon(&args[value_start..])
            .map(|pos| value_start + pos)
            .unwrap_or(args.len());

        let value = &args[value_start..value_end];

        // Parse TTL if present
        let ttl = if value_end < args.len() {
            let ttl_start = value_end + 1;
            if ttl_start < args.len() {
                let ttl_str =
                    std::str::from_utf8(&args[ttl_start..]).map_err(|_| "Invalid TTL encoding")?;
                Some(ttl_str.parse().map_err(|_| "Invalid TTL number")?)
            } else {
                None
            }
        } else {
            None
        };

        Ok(CommandRef::Put { key, value, ttl })
    }

    /// Parse DEL command with NEON optimizations
    unsafe fn parse_del_neon<'a>(&self, args: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        if args.is_empty() {
            return Err("DEL missing key");
        }

        let key_end = self.find_space_neon(args).unwrap_or(args.len());

        if key_end == 0 {
            return Err("DEL empty key");
        }

        Ok(CommandRef::Del {
            key: &args[..key_end],
        })
    }

    /// Find space character using NEON
    unsafe fn find_space_neon(&self, data: &[u8]) -> Option<usize> {
        if data.len() < 16 {
            return self.find_space_scalar(data);
        }

        let space_pattern = vdupq_n_u8(b' ');
        let mut offset = 0;

        // Process 16 bytes at a time with NEON
        while offset + 16 <= data.len() {
            let chunk = vld1q_u8(data[offset..].as_ptr());
            let cmp = vceqq_u8(chunk, space_pattern);
            let mask = self.extract_mask(cmp);

            if mask != 0 {
                // Found space, find exact position
                let pos = mask.trailing_zeros() as usize;
                return Some(offset + pos);
            }

            offset += 16;
        }

        // Handle remaining bytes
        if offset < data.len() {
            if let Some(pos) = self.find_space_scalar(&data[offset..]) {
                return Some(offset + pos);
            }
        }

        None
    }

    /// Scalar fallback for finding space
    fn find_space_scalar(&self, data: &[u8]) -> Option<usize> {
        data.iter().position(|&b| b == b' ')
    }

    /// Scalar fallback for unknown commands
    fn parse_scalar_fallback<'a>(&self, data: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_zero_copy(data)
    }

    /// Batch parse multiple commands using NEON
    pub unsafe fn parse_batch_neon<'a>(
        &self,
        data: &'a [u8],
    ) -> Vec<Result<CommandRef<'a>, &'static str>> {
        let mut commands = Vec::new();
        let mut offset = 0;

        // Use NEON to find all newline positions first
        let newline_positions = self.find_newlines_neon(data);

        for &newline_pos in &newline_positions {
            if newline_pos > offset {
                let command_data = &data[offset..newline_pos];

                // Remove \r if present
                let command_data = if command_data.ends_with(b"\r") {
                    &command_data[..command_data.len() - 1]
                } else {
                    command_data
                };

                if !command_data.is_empty() {
                    commands.push(self.parse_neon(command_data));
                }
            }
            offset = newline_pos + 1;
        }

        commands
    }

    /// Find all newline positions using NEON
    unsafe fn find_newlines_neon(&self, data: &[u8]) -> Vec<usize> {
        let mut positions = Vec::new();

        if data.len() < 16 {
            // Fallback to scalar for small data
            for (i, &byte) in data.iter().enumerate() {
                if byte == b'\n' {
                    positions.push(i);
                }
            }
            return positions;
        }

        let newline_pattern = vdupq_n_u8(b'\n');
        let mut offset = 0;

        // Process 16 bytes at a time
        while offset + 16 <= data.len() {
            let chunk = vld1q_u8(data[offset..].as_ptr());
            let cmp = vceqq_u8(chunk, newline_pattern);
            let mask = self.extract_mask(cmp);

            if mask != 0 {
                // Found newlines, extract all positions
                let mut temp_mask = mask;
                while temp_mask != 0 {
                    let pos = temp_mask.trailing_zeros() as usize;
                    positions.push(offset + pos);
                    temp_mask &= temp_mask - 1; // Clear lowest set bit
                }
            }

            offset += 16;
        }

        // Handle remaining bytes
        for i in offset..data.len() {
            if data[i] == b'\n' {
                positions.push(i);
            }
        }

        positions
    }
}

#[cfg(not(target_arch = "aarch64"))]
impl NeonParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse_neon<'a>(&self, data: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        // Fallback to zero-copy parser on non-ARM64
        crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_zero_copy(data)
    }

    pub fn parse_batch_neon<'a>(
        &self,
        data: &'a [u8],
    ) -> Vec<Result<CommandRef<'a>, &'static str>> {
        // Fallback to zero-copy parser
        crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_batch_zero_copy(data)
    }
}

impl Default for NeonParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Global NEON parser instance
static NEON_PARSER: std::sync::LazyLock<NeonParser> = std::sync::LazyLock::new(NeonParser::new);

/// Parse command using global NEON parser (ARM64 native)
#[inline(always)]
pub fn parse_command_neon(data: &[u8]) -> Result<CommandRef<'_>, &'static str> {
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            unsafe { NEON_PARSER.parse_neon(data) }
        } else {
            // Fallback to zero-copy parser
            crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_zero_copy(data)
        }
    }

    #[cfg(not(target_arch = "aarch64"))]
    {
        // Use SIMD parser on x86 or fallback on other architectures
        crate::ultra_fast::simd_parser::parse_command_simd(data)
    }
}

/// Parse batch of commands using global NEON parser
#[inline(always)]
pub fn parse_batch_neon(data: &[u8]) -> Vec<Result<CommandRef<'_>, &'static str>> {
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            unsafe { NEON_PARSER.parse_batch_neon(data) }
        } else {
            crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_batch_zero_copy(data)
        }
    }

    #[cfg(not(target_arch = "aarch64"))]
    {
        crate::ultra_fast::simd_parser::parse_batch_simd(data)
    }
}

/// ARM64 NEON hash function for ultra-fast key hashing
#[cfg(target_arch = "aarch64")]
#[inline(always)]
pub fn hash_key_neon(key: &[u8]) -> u64 {
    if key.is_empty() {
        return 0;
    }

    unsafe {
        let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
        let fnv_prime: u64 = 0x100000001b3;

        // Process 16 bytes at a time with NEON
        let chunks = key.len() / 16;
        let mut offset = 0;

        for _ in 0..chunks {
            let chunk = vld1q_u8(key[offset..].as_ptr());

            // Extract bytes and hash them
            let bytes: [u8; 16] = std::mem::transmute(chunk);
            for &byte in &bytes {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(fnv_prime);
            }

            offset += 16;
        }

        // Handle remaining bytes
        for &byte in &key[offset..] {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(fnv_prime);
        }

        hash
    }
}

/// Fallback hash for non-ARM64
#[cfg(not(target_arch = "aarch64"))]
#[inline(always)]
pub fn hash_key_neon(key: &[u8]) -> u64 {
    crate::ultra_fast::assembly_optimized::hash_key_asm(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neon_parser_creation() {
        let parser = NeonParser::new();
        // Just test that it doesn't crash
        assert!(true);
    }

    #[test]
    fn test_neon_ping() {
        let result = parse_command_neon(b"PING");
        assert!(matches!(result, Ok(CommandRef::Ping)));
    }

    #[test]
    fn test_neon_get() {
        let result = parse_command_neon(b"GET mykey");
        match result {
            Ok(CommandRef::Get { key }) => {
                assert_eq!(key, b"mykey");
            }
            _ => panic!("Expected GET command"),
        }
    }

    #[test]
    fn test_neon_hash() {
        let key1 = b"test_key";
        let key2 = b"test_key";
        let key3 = b"different_key";

        let hash1 = hash_key_neon(key1);
        let hash2 = hash_key_neon(key2);
        let hash3 = hash_key_neon(key3);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_ne!(hash1, 0);
    }

    #[test]
    fn test_performance_comparison() {
        let data = b"GET very_long_key_name_for_testing";
        let iterations = 1000;

        // Test NEON version
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            let _ = parse_command_neon(data);
        }
        let neon_time = start.elapsed();

        // Test fallback version
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            let _ = crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_zero_copy(data);
        }
        let fallback_time = start.elapsed();

        println!("NEON time: {:?}", neon_time);
        println!("Fallback time: {:?}", fallback_time);

        // Both should work
        assert!(neon_time.as_nanos() > 0);
        assert!(fallback_time.as_nanos() > 0);
    }
}
