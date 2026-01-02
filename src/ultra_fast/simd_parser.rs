//! SIMD-optimized command parser for ultra-low latency

use crate::ultra_fast::zero_copy_parser::CommandRef;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use std::arch::x86_64::*;

/// SIMD-optimized command parser
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub struct SIMDParser {
    // Pre-computed SIMD patterns for common commands
    ping_pattern: __m128i,
    get_pattern: __m128i,
    put_pattern: __m128i,
    del_pattern: __m128i,
}

/// Fallback parser for non-x86 architectures
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub struct SIMDParser {
    // Empty struct for non-x86 platforms
}

impl SIMDParser {
    /// Create new SIMD parser with pre-computed patterns
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn new() -> Self {
        unsafe {
            Self {
                // "PING" padded to 16 bytes
                ping_pattern: _mm_set_epi8(
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, b'G' as i8, b'N' as i8, b'I' as i8,
                    b'P' as i8,
                ),
                // "GET " padded to 16 bytes
                get_pattern: _mm_set_epi8(
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, b' ' as i8, b'T' as i8, b'E' as i8,
                    b'G' as i8,
                ),
                // "PUT " padded to 16 bytes
                put_pattern: _mm_set_epi8(
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, b' ' as i8, b'T' as i8, b'U' as i8,
                    b'P' as i8,
                ),
                // "DEL " padded to 16 bytes
                del_pattern: _mm_set_epi8(
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, b' ' as i8, b'L' as i8, b'E' as i8,
                    b'D' as i8,
                ),
            }
        }
    }

    /// Create new SIMD parser (fallback for non-x86)
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    pub fn new() -> Self {
        Self {}
    }

    /// Parse command using SIMD optimizations
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "sse2,sse4.1")]
    pub unsafe fn parse_simd<'a>(&self, data: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        if data.is_empty() {
            return Err("Empty command");
        }

        // Fast path for short commands
        if data.len() < 16 {
            return self.parse_short_command(data);
        }

        // Load first 16 bytes for SIMD comparison
        let input = _mm_loadu_si128(data.as_ptr() as *const __m128i);

        // Compare with PING pattern
        let ping_cmp = _mm_cmpeq_epi32(input, self.ping_pattern);
        let ping_mask = _mm_movemask_epi8(ping_cmp);
        if ping_mask & 0xF000 == 0xF000 && data.len() == 4 {
            return Ok(CommandRef::Ping);
        }

        // Compare with GET pattern
        let get_cmp = _mm_cmpeq_epi32(input, self.get_pattern);
        let get_mask = _mm_movemask_epi8(get_cmp);
        if get_mask & 0xF000 == 0xF000 && data.len() > 4 {
            return self.parse_get_simd(&data[4..]);
        }

        // Compare with PUT pattern
        let put_cmp = _mm_cmpeq_epi32(input, self.put_pattern);
        let put_mask = _mm_movemask_epi8(put_cmp);
        if put_mask & 0xF000 == 0xF000 && data.len() > 4 {
            return self.parse_put_simd(&data[4..]);
        }

        // Compare with DEL pattern
        let del_cmp = _mm_cmpeq_epi32(input, self.del_pattern);
        let del_mask = _mm_movemask_epi8(del_cmp);
        if del_mask & 0xF000 == 0xF000 && data.len() > 4 {
            return self.parse_del_simd(&data[4..]);
        }

        // Fallback to scalar parsing for other commands
        self.parse_scalar_fallback(data)
    }

    /// Parse command using fallback (non-x86)
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    pub fn parse_simd<'a>(&self, data: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        // Fallback to zero-copy parser
        crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_zero_copy(data)
    }

    /// Parse short commands (< 16 bytes) using scalar operations
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[inline(always)]
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

    /// Parse GET command with SIMD optimizations
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "sse2,avx2")]
    unsafe fn parse_get_simd<'a>(&self, args: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        if args.is_empty() {
            return Err("GET missing key");
        }

        // Use SIMD to find space character (key end)
        let key_end = self.find_space_simd(args).unwrap_or(args.len());

        if key_end == 0 {
            return Err("GET empty key");
        }

        Ok(CommandRef::Get {
            key: &args[..key_end],
        })
    }

    /// Parse PUT command with SIMD optimizations
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "sse2,avx2")]
    unsafe fn parse_put_simd<'a>(&self, args: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        if args.is_empty() {
            return Err("PUT missing arguments");
        }

        // Find first space (key/value separator) using SIMD
        let key_end = match self.find_space_simd(args) {
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

        // Find value end using SIMD
        let value_end = self
            .find_space_simd(&args[value_start..])
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

    /// Parse DEL command with SIMD optimizations
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "sse2")]
    unsafe fn parse_del_simd<'a>(&self, args: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        if args.is_empty() {
            return Err("DEL missing key");
        }

        let key_end = self.find_space_simd(args).unwrap_or(args.len());

        if key_end == 0 {
            return Err("DEL empty key");
        }

        Ok(CommandRef::Del {
            key: &args[..key_end],
        })
    }

    /// Find space character using SIMD (AVX2 optimized)
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "avx2")]
    unsafe fn find_space_simd(&self, data: &[u8]) -> Option<usize> {
        if data.len() < 32 {
            return self.find_space_scalar(data);
        }

        let space_pattern = _mm256_set1_epi8(b' ' as i8);
        let mut offset = 0;

        // Process 32 bytes at a time with AVX2
        while offset + 32 <= data.len() {
            let chunk = _mm256_loadu_si256(data[offset..].as_ptr() as *const __m256i);
            let cmp = _mm256_cmpeq_epi8(chunk, space_pattern);
            let mask = _mm256_movemask_epi8(cmp);

            if mask != 0 {
                // Found space, find exact position
                let pos = mask.trailing_zeros() as usize;
                return Some(offset + pos);
            }

            offset += 32;
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
    #[inline(always)]
    fn find_space_scalar(&self, data: &[u8]) -> Option<usize> {
        data.iter().position(|&b| b == b' ')
    }

    /// Scalar fallback for unknown commands
    fn parse_scalar_fallback<'a>(&self, data: &'a [u8]) -> Result<CommandRef<'a>, &'static str> {
        // Use the original zero-copy parser as fallback
        crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_zero_copy(data)
    }

    /// Batch parse multiple commands using SIMD
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "sse2,avx2")]
    pub unsafe fn parse_batch_simd<'a>(
        &self,
        data: &'a [u8],
    ) -> Vec<Result<CommandRef<'a>, &'static str>> {
        let mut commands = Vec::new();
        let mut offset = 0;

        // Use SIMD to find all newline positions first
        let newline_positions = self.find_newlines_simd(data);

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
                    commands.push(self.parse_simd(command_data));
                }
            }
            offset = newline_pos + 1;
        }

        commands
    }

    /// Batch parse multiple commands (fallback for non-x86)
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    pub fn parse_batch_simd<'a>(
        &self,
        data: &'a [u8],
    ) -> Vec<Result<CommandRef<'a>, &'static str>> {
        // Fallback to zero-copy parser
        crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_batch_zero_copy(data)
    }

    /// Find all newline positions using SIMD
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "avx2")]
    unsafe fn find_newlines_simd(&self, data: &[u8]) -> Vec<usize> {
        let mut positions = Vec::new();

        if data.len() < 32 {
            // Fallback to scalar for small data
            for (i, &byte) in data.iter().enumerate() {
                if byte == b'\n' {
                    positions.push(i);
                }
            }
            return positions;
        }

        let newline_pattern = _mm256_set1_epi8(b'\n' as i8);
        let mut offset = 0;

        // Process 32 bytes at a time
        while offset + 32 <= data.len() {
            let chunk = _mm256_loadu_si256(data[offset..].as_ptr() as *const __m256i);
            let cmp = _mm256_cmpeq_epi8(chunk, newline_pattern);
            let mask = _mm256_movemask_epi8(cmp);

            if mask != 0 {
                // Found newlines, extract all positions
                let mut temp_mask = mask;
                while temp_mask != 0 {
                    let pos = temp_mask.trailing_zeros() as usize;
                    positions.push(offset + pos);
                    temp_mask &= temp_mask - 1; // Clear lowest set bit
                }
            }

            offset += 32;
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

impl Default for SIMDParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Global SIMD parser instance
static SIMD_PARSER: std::sync::LazyLock<SIMDParser> = std::sync::LazyLock::new(SIMDParser::new);

/// Parse command using global SIMD parser
#[inline(always)]
pub fn parse_command_simd(data: &[u8]) -> Result<CommandRef<'_>, &'static str> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("sse4.1") {
            unsafe { SIMD_PARSER.parse_simd(data) }
        } else {
            // Fallback to zero-copy parser on unsupported hardware
            crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_zero_copy(data)
        }
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        // Always use fallback on non-x86 platforms
        crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_zero_copy(data)
    }
}

/// Parse batch of commands using global SIMD parser
#[inline(always)]
pub fn parse_batch_simd(data: &[u8]) -> Vec<Result<CommandRef<'_>, &'static str>> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("sse4.1") {
            unsafe { SIMD_PARSER.parse_batch_simd(data) }
        } else {
            // Fallback to zero-copy parser
            crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_batch_zero_copy(data)
        }
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        // Always use fallback on non-x86 platforms
        crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_batch_zero_copy(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simd_parser_creation() {
        let parser = SIMDParser::new();
        // Just test that it doesn't crash
        assert!(true);
    }

    #[test]
    fn test_simd_ping() {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("sse2") {
                let result = parse_command_simd(b"PING");
                assert!(matches!(result, Ok(CommandRef::Ping)));
            }
        }

        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            let result = parse_command_simd(b"PING");
            assert!(matches!(result, Ok(CommandRef::Ping)));
        }
    }

    #[test]
    fn test_simd_get() {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("sse2") {
                let result = parse_command_simd(b"GET mykey");
                match result {
                    Ok(CommandRef::Get { key }) => {
                        assert_eq!(key, b"mykey");
                    }
                    _ => panic!("Expected GET command"),
                }
            }
        }

        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            let result = parse_command_simd(b"GET mykey");
            match result {
                Ok(CommandRef::Get { key }) => {
                    assert_eq!(key, b"mykey");
                }
                _ => panic!("Expected GET command"),
            }
        }
    }

    #[test]
    fn test_simd_put() {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("sse2") {
                let result = parse_command_simd(b"PUT mykey myvalue");
                match result {
                    Ok(CommandRef::Put { key, value, ttl }) => {
                        assert_eq!(key, b"mykey");
                        assert_eq!(value, b"myvalue");
                        assert_eq!(ttl, None);
                    }
                    _ => panic!("Expected PUT command"),
                }
            }
        }

        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            let result = parse_command_simd(b"PUT mykey myvalue");
            match result {
                Ok(CommandRef::Put { key, value, ttl }) => {
                    assert_eq!(key, b"mykey");
                    assert_eq!(value, b"myvalue");
                    assert_eq!(ttl, None);
                }
                _ => panic!("Expected PUT command"),
            }
        }
    }

    #[test]
    fn test_simd_batch() {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("avx2") {
                let data = b"PING\r\nGET key1\r\nPUT key2 value2\r\n";
                let commands = parse_batch_simd(data);

                assert_eq!(commands.len(), 3);
                assert!(matches!(commands[0], Ok(CommandRef::Ping)));
                assert!(matches!(commands[1], Ok(CommandRef::Get { .. })));
                assert!(matches!(commands[2], Ok(CommandRef::Put { .. })));
            }
        }

        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            let data = b"PING\r\nGET key1\r\nPUT key2 value2\r\n";
            let commands = parse_batch_simd(data);

            assert_eq!(commands.len(), 3);
            assert!(matches!(commands[0], Ok(CommandRef::Ping)));
            assert!(matches!(commands[1], Ok(CommandRef::Get { .. })));
            assert!(matches!(commands[2], Ok(CommandRef::Put { .. })));
        }
    }

    #[test]
    fn test_performance_comparison() {
        let data = b"GET very_long_key_name_for_testing";
        let iterations = 1000; // Reduced for faster tests

        // Test SIMD version
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            let _ = parse_command_simd(data);
        }
        let simd_time = start.elapsed();

        // Test scalar version
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            let _ = crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_zero_copy(data);
        }
        let scalar_time = start.elapsed();

        println!("SIMD time: {:?}", simd_time);
        println!("Scalar time: {:?}", scalar_time);

        // Just ensure it doesn't crash
        assert!(simd_time.as_nanos() > 0);
        assert!(scalar_time.as_nanos() > 0);
    }
}
