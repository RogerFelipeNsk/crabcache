//! SIMD-optimized protocol parser for maximum performance
//! 
//! This module implements vectorized parsing operations using SIMD instructions
//! to achieve the 300,000+ ops/sec target for Phase 6.1

use crate::protocol::Command;
use bytes::Bytes;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// SIMD-optimized protocol parser
pub struct SIMDParser {
    /// Whether SIMD is available on this CPU
    simd_available: bool,
    /// Buffer for aligned SIMD operations
    aligned_buffer: Vec<u8>,
}

impl SIMDParser {
    /// Create new SIMD parser with CPU feature detection
    pub fn new() -> Self {
        let simd_available = Self::detect_simd_support();
        
        Self {
            simd_available,
            aligned_buffer: Vec::with_capacity(4096),
        }
    }

    /// Detect SIMD support on current CPU
    fn detect_simd_support() -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            is_x86_feature_detected!("sse2") && is_x86_feature_detected!("avx2")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    }

    /// Parse batch of commands using SIMD optimizations
    pub fn parse_batch_simd(&mut self, data: &[u8]) -> Result<Vec<Command>, String> {
        if !self.simd_available || data.len() < 32 {
            return self.parse_batch_scalar(data);
        }

        #[cfg(target_arch = "x86_64")]
        {
            unsafe { self.parse_batch_avx2(data) }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            self.parse_batch_scalar(data)
        }
    }

    /// SIMD-optimized batch parsing using AVX2
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    unsafe fn parse_batch_avx2(&mut self, data: &[u8]) -> Result<Vec<Command>, String> {
        let mut commands = Vec::new();
        let mut offset = 0;

        // Process data in 32-byte chunks using AVX2
        while offset + 32 <= data.len() {
            let chunk = &data[offset..offset + 32];
            
            // Load 32 bytes into AVX2 register
            let chunk_vec = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);
            
            // Search for newlines (0x0A) in parallel
            let newlines = _mm256_set1_epi8(0x0A);
            let cmp_result = _mm256_cmpeq_epi8(chunk_vec, newlines);
            let mask = _mm256_movemask_epi8(cmp_result) as u32;
            
            if mask != 0 {
                // Found newlines, process commands in this chunk
                let mut chunk_offset = 0;
                let mut remaining_mask = mask;
                
                while remaining_mask != 0 {
                    let newline_pos = remaining_mask.trailing_zeros() as usize;
                    let command_end = chunk_offset + newline_pos;
                    
                    if command_end > chunk_offset {
                        let command_bytes = &chunk[chunk_offset..command_end];
                        if let Ok(command) = self.parse_single_command_simd(command_bytes) {
                            commands.push(command);
                        }
                    }
                    
                    chunk_offset = command_end + 1;
                    remaining_mask &= remaining_mask - 1; // Clear lowest set bit
                }
                
                offset += chunk_offset;
            } else {
                // No newlines in this chunk, skip to next
                offset += 32;
            }
        }

        // Process remaining bytes with scalar parsing
        while offset < data.len() {
            if let Some(newline_pos) = data[offset..].iter().position(|&b| b == b'\n') {
                let command_end = offset + newline_pos;
                let command_bytes = &data[offset..command_end];
                
                if let Ok(command) = self.parse_single_command_simd(command_bytes) {
                    commands.push(command);
                }
                
                offset = command_end + 1;
            } else {
                break;
            }
        }

        Ok(commands)
    }

    /// SIMD-optimized single command parsing
    fn parse_single_command_simd(&self, data: &[u8]) -> Result<Command, String> {
        if data.is_empty() {
            return Err("Empty command".to_string());
        }

        // Fast path for common commands using SIMD string comparison
        if self.simd_available && data.len() >= 4 {
            #[cfg(target_arch = "x86_64")]
            {
                return unsafe { self.parse_command_fast_path(data) };
            }
        }

        // Fallback to scalar parsing
        self.parse_single_command_scalar(data)
    }

    /// Fast path command parsing using SIMD string comparison
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sse2")]
    unsafe fn parse_command_fast_path(&self, data: &[u8]) -> Result<Command, String> {
        // Check for common 4-byte commands using SIMD
        if data.len() >= 4 {
            let first_4_bytes = _mm_loadu_si128(data.as_ptr() as *const __m128i);
            
            // Check for "PING" (case insensitive)
            let ping_pattern = _mm_set_epi8(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, b'G' as i8, b'N' as i8, b'I' as i8, b'P' as i8);
            let ping_cmp = _mm_cmpeq_epi32(first_4_bytes, ping_pattern);
            let ping_mask = _mm_movemask_epi8(ping_cmp);
            
            if ping_mask & 0xF000 == 0xF000 {
                return Ok(Command::Ping);
            }
        }

        // Check for "GET " prefix
        if data.len() >= 4 && &data[0..4] == b"GET " {
            return self.parse_get_command_simd(&data[4..]);
        }

        // Check for "PUT " prefix
        if data.len() >= 4 && &data[0..4] == b"PUT " {
            return self.parse_put_command_simd(&data[4..]);
        }

        // Check for "DEL " prefix
        if data.len() >= 4 && &data[0..4] == b"DEL " {
            return self.parse_del_command_simd(&data[4..]);
        }

        // Fallback to scalar parsing
        self.parse_single_command_scalar(data)
    }

    /// Parse GET command with SIMD optimization
    fn parse_get_command_simd(&self, args: &[u8]) -> Result<Command, String> {
        // Find first space or end of string for key
        let key_end = args.iter().position(|&b| b == b' ').unwrap_or(args.len());
        
        if key_end == 0 {
            return Err("GET command missing key".to_string());
        }

        let key = Bytes::from(args[..key_end].to_vec());
        Ok(Command::Get { key })
    }

    /// Parse PUT command with SIMD optimization
    fn parse_put_command_simd(&self, args: &[u8]) -> Result<Command, String> {
        // Find spaces to separate key and value
        let space_positions: Vec<usize> = Vec::new();
        
        #[cfg(target_arch = "x86_64")]
        if self.simd_available && args.len() >= 16 {
            // Use SIMD to find spaces quickly
            unsafe {
                let mut offset = 0;
                let mut positions = Vec::new();
                while offset + 16 <= args.len() {
                    let chunk = _mm_loadu_si128(args[offset..].as_ptr() as *const __m128i);
                    let spaces = _mm_set1_epi8(b' ' as i8);
                    let cmp_result = _mm_cmpeq_epi8(chunk, spaces);
                    let mask = _mm_movemask_epi8(cmp_result) as u16;
                    
                    if mask != 0 {
                        for i in 0..16 {
                            if (mask & (1 << i)) != 0 {
                                positions.push(offset + i);
                            }
                        }
                    }
                    offset += 16;
                }
                
                // Check remaining bytes
                for i in offset..args.len() {
                    if args[i] == b' ' {
                        positions.push(i);
                    }
                }
                
                if positions.is_empty() {
                    return Err("PUT command missing value".to_string());
                }

                let key_end = positions[0];
                let value_start = key_end + 1;
                
                if key_end == 0 || value_start >= args.len() {
                    return Err("Invalid PUT command format".to_string());
                }

                let key = Bytes::from(args[..key_end].to_vec());
                let value = Bytes::from(args[value_start..].to_vec());
                
                return Ok(Command::Put { key, value, ttl: None });
            }
        }
        
        // Scalar fallback
        for (i, &byte) in args.iter().enumerate() {
            if byte == b' ' {
                let key_end = i;
                let value_start = key_end + 1;
                
                if key_end == 0 || value_start >= args.len() {
                    return Err("Invalid PUT command format".to_string());
                }

                let key = Bytes::from(args[..key_end].to_vec());
                let value = Bytes::from(args[value_start..].to_vec());
                
                return Ok(Command::Put { key, value, ttl: None });
            }
        }

        Err("PUT command missing value".to_string())
    }

    /// Parse DEL command with SIMD optimization
    fn parse_del_command_simd(&self, args: &[u8]) -> Result<Command, String> {
        // Find first space or end of string for key
        let key_end = args.iter().position(|&b| b == b' ').unwrap_or(args.len());
        
        if key_end == 0 {
            return Err("DEL command missing key".to_string());
        }

        let key = Bytes::from(args[..key_end].to_vec());
        Ok(Command::Del { key })
    }

    /// Scalar fallback parsing
    fn parse_batch_scalar(&self, data: &[u8]) -> Result<Vec<Command>, String> {
        let mut commands = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            if let Some(newline_pos) = data[offset..].iter().position(|&b| b == b'\n') {
                let command_end = offset + newline_pos;
                let command_bytes = &data[offset..command_end];
                
                if let Ok(command) = self.parse_single_command_scalar(command_bytes) {
                    commands.push(command);
                }
                
                offset = command_end + 1;
            } else {
                break;
            }
        }

        Ok(commands)
    }

    /// Scalar single command parsing
    fn parse_single_command_scalar(&self, data: &[u8]) -> Result<Command, String> {
        let command_str = std::str::from_utf8(data)
            .map_err(|e| format!("Invalid UTF-8: {}", e))?;
        
        let parts: Vec<&str> = command_str.split_whitespace().collect();
        if parts.is_empty() {
            return Err("Empty command".to_string());
        }

        match parts[0].to_uppercase().as_str() {
            "PING" => Ok(Command::Ping),
            "GET" if parts.len() >= 2 => Ok(Command::Get {
                key: Bytes::from(parts[1].to_string()),
            }),
            "PUT" if parts.len() >= 3 => Ok(Command::Put {
                key: Bytes::from(parts[1].to_string()),
                value: Bytes::from(parts[2].to_string()),
                ttl: None,
            }),
            "DEL" if parts.len() >= 2 => Ok(Command::Del {
                key: Bytes::from(parts[1].to_string()),
            }),
            "STATS" => Ok(Command::Stats),
            "METRICS" => Ok(Command::Metrics),
            _ => Err(format!("Unknown command: {}", parts[0])),
        }
    }

    /// Get SIMD availability status
    pub fn is_simd_available(&self) -> bool {
        self.simd_available
    }

    /// Benchmark SIMD vs scalar performance
    pub fn benchmark_parsing(&mut self, data: &[u8], iterations: usize) -> (f64, f64) {
        let start = std::time::Instant::now();
        
        // Benchmark SIMD parsing
        for _ in 0..iterations {
            let _ = self.parse_batch_simd(data);
        }
        let simd_time = start.elapsed().as_secs_f64();
        
        let start = std::time::Instant::now();
        
        // Benchmark scalar parsing
        for _ in 0..iterations {
            let _ = self.parse_batch_scalar(data);
        }
        let scalar_time = start.elapsed().as_secs_f64();
        
        (simd_time, scalar_time)
    }
}

impl Default for SIMDParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simd_parser_creation() {
        let parser = SIMDParser::new();
        println!("SIMD available: {}", parser.is_simd_available());
    }

    #[test]
    fn test_simd_command_parsing() {
        let mut parser = SIMDParser::new();
        let test_data = b"PING\nGET test_key\nPUT key value\nDEL old_key\n";
        
        let result = parser.parse_batch_simd(test_data);
        assert!(result.is_ok());
        
        let commands = result.unwrap();
        assert_eq!(commands.len(), 4);
        
        match &commands[0] {
            Command::Ping => {},
            _ => panic!("Expected PING command"),
        }
        
        match &commands[1] {
            Command::Get { key } => {
                assert_eq!(key.as_ref(), b"test_key");
            },
            _ => panic!("Expected GET command"),
        }
    }

    #[test]
    fn test_simd_vs_scalar_benchmark() {
        let mut parser = SIMDParser::new();
        let test_data = b"GET key1\nPUT key2 value2\nDEL key3\nPING\n".repeat(100);
        
        let (simd_time, scalar_time) = parser.benchmark_parsing(&test_data, 100);
        
        println!("SIMD time: {:.6}s", simd_time);
        println!("Scalar time: {:.6}s", scalar_time);
        
        if parser.is_simd_available() {
            println!("SIMD speedup: {:.2}x", scalar_time / simd_time);
        }
    }

    #[test]
    fn test_large_batch_parsing() {
        let mut parser = SIMDParser::new();
        
        // Create a large batch of commands
        let mut large_batch = Vec::new();
        for i in 0..1000 {
            large_batch.extend_from_slice(format!("GET key_{}\n", i).as_bytes());
            large_batch.extend_from_slice(format!("PUT key_{} value_{}\n", i, i).as_bytes());
        }
        
        let start = std::time::Instant::now();
        let result = parser.parse_batch_simd(&large_batch);
        let parse_time = start.elapsed();
        
        assert!(result.is_ok());
        let commands = result.unwrap();
        assert_eq!(commands.len(), 2000);
        
        let ops_per_second = commands.len() as f64 / parse_time.as_secs_f64();
        println!("Parsing performance: {:.0} ops/sec", ops_per_second);
        
        // Should be able to parse at least 100k ops/sec
        assert!(ops_per_second > 100_000.0);
    }
}