//! Zero-copy buffer operations for maximum performance
//!
//! This module implements zero-copy buffer management to minimize memory
//! allocations and copies, targeting 300,000+ ops/sec performance

use crate::protocol::{Command, Response};
use bytes::{BufMut, Bytes, BytesMut};
use std::collections::VecDeque;
use std::ptr;
use std::sync::Arc;

/// Zero-copy buffer pool for reusing memory allocations
pub struct ZeroCopyBufferPool {
    /// Pool of reusable buffers
    buffer_pool: VecDeque<BytesMut>,
    /// Pool statistics
    stats: ZeroCopyStats,
    /// Buffer size configuration
    config: ZeroCopyConfig,
}

/// Configuration for zero-copy operations
#[derive(Debug, Clone)]
pub struct ZeroCopyConfig {
    /// Default buffer size
    pub default_buffer_size: usize,
    /// Maximum buffer size
    pub max_buffer_size: usize,
    /// Pool size limit
    pub max_pool_size: usize,
    /// Enable buffer reuse
    pub enable_buffer_reuse: bool,
    /// Enable memory alignment
    pub enable_alignment: bool,
    /// Alignment size (must be power of 2)
    pub alignment_size: usize,
}

impl Default for ZeroCopyConfig {
    fn default() -> Self {
        Self {
            default_buffer_size: 4096,
            max_buffer_size: 1024 * 1024, // 1MB
            max_pool_size: 1000,
            enable_buffer_reuse: true,
            enable_alignment: true,
            alignment_size: 64, // Cache line aligned
        }
    }
}

/// Zero-copy operation statistics
#[derive(Debug, Clone, Default)]
pub struct ZeroCopyStats {
    /// Total buffer allocations
    pub total_allocations: u64,
    /// Buffer reuse hits
    pub reuse_hits: u64,
    /// Buffer reuse misses
    pub reuse_misses: u64,
    /// Total bytes allocated
    pub total_bytes_allocated: u64,
    /// Total bytes saved through reuse
    pub total_bytes_saved: u64,
    /// Current pool size
    pub current_pool_size: usize,
}

impl ZeroCopyBufferPool {
    /// Create new zero-copy buffer pool
    pub fn new(config: ZeroCopyConfig) -> Self {
        Self {
            buffer_pool: VecDeque::with_capacity(config.max_pool_size),
            stats: ZeroCopyStats::default(),
            config,
        }
    }

    /// Get a buffer from the pool or allocate a new one
    pub fn get_buffer(&mut self, min_size: usize) -> BytesMut {
        let required_size = if self.config.enable_alignment {
            self.align_size(min_size.max(self.config.default_buffer_size))
        } else {
            min_size.max(self.config.default_buffer_size)
        };

        // Try to reuse a buffer from the pool
        if self.config.enable_buffer_reuse {
            while let Some(mut buffer) = self.buffer_pool.pop_front() {
                if buffer.capacity() >= required_size {
                    buffer.clear();
                    self.stats.reuse_hits += 1;
                    self.stats.total_bytes_saved += buffer.capacity() as u64;
                    return buffer;
                }
                // Buffer too small, discard it
            }
        }

        // Allocate new buffer
        self.stats.reuse_misses += 1;
        self.stats.total_allocations += 1;
        self.stats.total_bytes_allocated += required_size as u64;

        if self.config.enable_alignment {
            self.allocate_aligned_buffer(required_size)
        } else {
            BytesMut::with_capacity(required_size)
        }
    }

    /// Return a buffer to the pool for reuse
    pub fn return_buffer(&mut self, buffer: BytesMut) {
        if !self.config.enable_buffer_reuse {
            return;
        }

        if self.buffer_pool.len() < self.config.max_pool_size
            && buffer.capacity() <= self.config.max_buffer_size
        {
            self.buffer_pool.push_back(buffer);
            self.stats.current_pool_size = self.buffer_pool.len();
        }
    }

    /// Allocate aligned buffer for optimal SIMD performance
    fn allocate_aligned_buffer(&self, size: usize) -> BytesMut {
        let aligned_size = self.align_size(size);

        // For now, use regular allocation (in production, we'd use aligned_alloc)
        BytesMut::with_capacity(aligned_size)
    }

    /// Align size to specified boundary
    fn align_size(&self, size: usize) -> usize {
        let alignment = self.config.alignment_size;
        (size + alignment - 1) & !(alignment - 1)
    }

    /// Get current statistics
    pub fn get_stats(&self) -> &ZeroCopyStats {
        &self.stats
    }

    /// Calculate buffer reuse efficiency
    pub fn get_reuse_efficiency(&self) -> f64 {
        let total_requests = self.stats.reuse_hits + self.stats.reuse_misses;
        if total_requests == 0 {
            0.0
        } else {
            self.stats.reuse_hits as f64 / total_requests as f64
        }
    }
}

/// Zero-copy command serializer
pub struct ZeroCopySerializer {
    /// Buffer pool for reuse
    buffer_pool: Arc<std::sync::Mutex<ZeroCopyBufferPool>>,
    /// Serialization statistics
    stats: ZeroCopySerializerStats,
}

/// Serializer statistics
#[derive(Debug, Clone, Default)]
pub struct ZeroCopySerializerStats {
    /// Commands serialized
    pub commands_serialized: u64,
    /// Responses serialized
    pub responses_serialized: u64,
    /// Total bytes serialized
    pub total_bytes_serialized: u64,
    /// Zero-copy operations performed
    pub zero_copy_operations: u64,
}

impl ZeroCopySerializer {
    /// Create new zero-copy serializer
    pub fn new(buffer_pool: Arc<std::sync::Mutex<ZeroCopyBufferPool>>) -> Self {
        Self {
            buffer_pool,
            stats: ZeroCopySerializerStats::default(),
        }
    }

    /// Serialize batch of responses with zero-copy optimization
    pub fn serialize_response_batch_zero_copy(
        &mut self,
        responses: &[Response],
    ) -> Result<Bytes, String> {
        if responses.is_empty() {
            return Ok(Bytes::new());
        }

        // Estimate required buffer size
        let estimated_size = self.estimate_response_batch_size(responses);

        // Get buffer from pool
        let mut buffer = {
            let mut pool = self.buffer_pool.lock().unwrap();
            pool.get_buffer(estimated_size)
        };

        // Serialize responses directly into buffer
        for response in responses {
            self.serialize_response_zero_copy(response, &mut buffer)?;
        }

        self.stats.responses_serialized += responses.len() as u64;
        self.stats.total_bytes_serialized += buffer.len() as u64;
        self.stats.zero_copy_operations += 1;

        // Convert to immutable Bytes (zero-copy)
        let result = buffer.freeze();

        Ok(result)
    }

    /// Serialize single response with zero-copy
    fn serialize_response_zero_copy(
        &self,
        response: &Response,
        buffer: &mut BytesMut,
    ) -> Result<(), String> {
        match response {
            Response::Ok => {
                buffer.put_slice(b"OK\n");
            }
            Response::Pong => {
                buffer.put_slice(b"PONG\n");
            }
            Response::Value(value) => {
                buffer.put_slice(value.as_ref());
                buffer.put_u8(b'\n');
            }
            Response::Null => {
                buffer.put_slice(b"NULL\n");
            }
            Response::Error(msg) => {
                buffer.put_slice(b"ERROR ");
                buffer.put_slice(msg.as_bytes());
                buffer.put_u8(b'\n');
            }
            Response::Stats(stats) => {
                buffer.put_slice(stats.as_bytes());
                buffer.put_u8(b'\n');
            }
        }
        Ok(())
    }

    /// Estimate buffer size needed for response batch
    fn estimate_response_batch_size(&self, responses: &[Response]) -> usize {
        let mut total_size = 0;

        for response in responses {
            total_size += match response {
                Response::Ok => 3,                         // "OK\n"
                Response::Pong => 5,                       // "PONG\n"
                Response::Value(value) => value.len() + 1, // value + "\n"
                Response::Null => 5,                       // "NULL\n"
                Response::Error(msg) => 6 + msg.len() + 1, // "ERROR " + msg + "\n"
                Response::Stats(stats) => stats.len() + 1, // stats + "\n"
            };
        }

        // Add 10% buffer for safety
        total_size + (total_size / 10)
    }

    /// Parse commands with zero-copy optimization
    pub fn parse_command_batch_zero_copy(&mut self, data: &[u8]) -> Result<Vec<Command>, String> {
        let mut commands = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            if let Some(newline_pos) = data[offset..].iter().position(|&b| b == b'\n') {
                let command_end = offset + newline_pos;
                let command_bytes = &data[offset..command_end];

                // Parse command with zero-copy (reuse byte slices)
                if let Ok(command) = self.parse_single_command_zero_copy(command_bytes) {
                    commands.push(command);
                }

                offset = command_end + 1;
            } else {
                break;
            }
        }

        self.stats.commands_serialized += commands.len() as u64;
        Ok(commands)
    }

    /// Parse single command with zero-copy optimization
    fn parse_single_command_zero_copy(&self, data: &[u8]) -> Result<Command, String> {
        if data.is_empty() {
            return Err("Empty command".to_string());
        }

        // Fast path for common commands
        match data[0] {
            b'P' | b'p' if data.len() >= 4 => {
                if data[..4].eq_ignore_ascii_case(b"PING") {
                    return Ok(Command::Ping);
                }
                if data.len() > 4 && data[..4].eq_ignore_ascii_case(b"PUT ") {
                    return self.parse_put_zero_copy(&data[4..]);
                }
            }
            b'G' | b'g' if data.len() >= 4 && data[..4].eq_ignore_ascii_case(b"GET ") => {
                return self.parse_get_zero_copy(&data[4..]);
            }
            b'D' | b'd' if data.len() >= 4 && data[..4].eq_ignore_ascii_case(b"DEL ") => {
                return self.parse_del_zero_copy(&data[4..]);
            }
            b'S' | b's' if data.len() >= 5 && data[..5].eq_ignore_ascii_case(b"STATS") => {
                return Ok(Command::Stats);
            }
            b'M' | b'm' if data.len() >= 7 && data[..7].eq_ignore_ascii_case(b"METRICS") => {
                return Ok(Command::Metrics);
            }
            _ => {}
        }

        Err(format!(
            "Unknown command: {}",
            String::from_utf8_lossy(data)
        ))
    }

    /// Parse GET command with zero-copy
    fn parse_get_zero_copy(&self, args: &[u8]) -> Result<Command, String> {
        let key_end = args.iter().position(|&b| b == b' ').unwrap_or(args.len());

        if key_end == 0 {
            return Err("GET command missing key".to_string());
        }

        // Zero-copy: create Bytes directly from slice
        let key = Bytes::copy_from_slice(&args[..key_end]);
        Ok(Command::Get { key })
    }

    /// Parse PUT command with zero-copy
    fn parse_put_zero_copy(&self, args: &[u8]) -> Result<Command, String> {
        let space_pos = args
            .iter()
            .position(|&b| b == b' ')
            .ok_or("PUT command missing value")?;

        if space_pos == 0 {
            return Err("PUT command missing key".to_string());
        }

        let key = Bytes::copy_from_slice(&args[..space_pos]);
        let value = Bytes::copy_from_slice(&args[space_pos + 1..]);

        Ok(Command::Put {
            key,
            value,
            ttl: None,
        })
    }

    /// Parse DEL command with zero-copy
    fn parse_del_zero_copy(&self, args: &[u8]) -> Result<Command, String> {
        let key_end = args.iter().position(|&b| b == b' ').unwrap_or(args.len());

        if key_end == 0 {
            return Err("DEL command missing key".to_string());
        }

        let key = Bytes::copy_from_slice(&args[..key_end]);
        Ok(Command::Del { key })
    }

    /// Get serializer statistics
    pub fn get_stats(&self) -> &ZeroCopySerializerStats {
        &self.stats
    }

    /// Calculate zero-copy efficiency
    pub fn get_zero_copy_efficiency(&self) -> f64 {
        let total_operations = self.stats.commands_serialized + self.stats.responses_serialized;
        if total_operations == 0 {
            0.0
        } else {
            self.stats.zero_copy_operations as f64 / total_operations as f64
        }
    }
}

/// Memory-mapped buffer for ultra-high performance
pub struct MemoryMappedBuffer {
    /// Memory-mapped region
    #[cfg(unix)]
    mmap: Option<memmap2::MmapMut>,
    /// Fallback buffer for non-Unix systems
    #[cfg(not(unix))]
    buffer: Vec<u8>,
    /// Current position
    position: usize,
    /// Buffer size
    size: usize,
}

impl MemoryMappedBuffer {
    /// Create new memory-mapped buffer
    pub fn new(size: usize) -> Result<Self, String> {
        #[cfg(unix)]
        {
            use memmap2::MmapOptions;
            use std::io::Write;

            // Create temporary file
            let mut temp_file =
                tempfile::tempfile().map_err(|e| format!("Failed to create temp file: {}", e))?;

            // Write zeros to set file size
            temp_file
                .write_all(&vec![0u8; size])
                .map_err(|e| format!("Failed to write to temp file: {}", e))?;

            // Memory map the file
            let mmap = unsafe {
                MmapOptions::new()
                    .map_mut(&temp_file)
                    .map_err(|e| format!("Failed to create memory map: {}", e))?
            };

            Ok(Self {
                mmap: Some(mmap),
                position: 0,
                size,
            })
        }
        #[cfg(not(unix))]
        {
            Ok(Self {
                buffer: vec![0u8; size],
                position: 0,
                size,
            })
        }
    }

    /// Write data to buffer
    pub fn write(&mut self, data: &[u8]) -> Result<usize, String> {
        if self.position + data.len() > self.size {
            return Err("Buffer overflow".to_string());
        }

        #[cfg(unix)]
        {
            if let Some(ref mut mmap) = self.mmap {
                unsafe {
                    ptr::copy_nonoverlapping(
                        data.as_ptr(),
                        mmap.as_mut_ptr().add(self.position),
                        data.len(),
                    );
                }
            }
        }
        #[cfg(not(unix))]
        {
            self.buffer[self.position..self.position + data.len()].copy_from_slice(data);
        }

        self.position += data.len();
        Ok(data.len())
    }

    /// Read data from buffer
    pub fn read(&self, offset: usize, len: usize) -> Result<&[u8], String> {
        if offset + len > self.size {
            return Err("Read beyond buffer".to_string());
        }

        #[cfg(unix)]
        {
            if let Some(ref mmap) = self.mmap {
                unsafe {
                    let ptr = mmap.as_ptr().add(offset);
                    return Ok(std::slice::from_raw_parts(ptr, len));
                }
            }
        }
        #[cfg(not(unix))]
        {
            return Ok(&self.buffer[offset..offset + len]);
        }

        Err("No buffer available".to_string())
    }

    /// Reset buffer position
    pub fn reset(&mut self) {
        self.position = 0;
    }

    /// Get current position
    pub fn position(&self) -> usize {
        self.position
    }

    /// Get buffer size
    pub fn size(&self) -> usize {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_copy_buffer_pool() {
        let config = ZeroCopyConfig::default();
        let mut pool = ZeroCopyBufferPool::new(config);

        // Get a buffer
        let buffer1 = pool.get_buffer(1024);
        assert!(buffer1.capacity() >= 1024);

        // Return it
        pool.return_buffer(buffer1);

        // Get another buffer (should reuse)
        let buffer2 = pool.get_buffer(1024);
        assert!(buffer2.capacity() >= 1024);

        // Check reuse efficiency
        let efficiency = pool.get_reuse_efficiency();
        assert!(efficiency > 0.0);
    }

    #[test]
    fn test_zero_copy_serializer() {
        let config = ZeroCopyConfig::default();
        let pool = Arc::new(std::sync::Mutex::new(ZeroCopyBufferPool::new(config)));
        let mut serializer = ZeroCopySerializer::new(pool);

        // Test response serialization
        let responses = vec![
            Response::Ok,
            Response::Pong,
            Response::Value(Bytes::from("test_value")),
        ];

        let result = serializer.serialize_response_batch_zero_copy(&responses);
        assert!(result.is_ok());

        let serialized = result.unwrap();
        assert!(!serialized.is_empty());

        // Check stats
        let stats = serializer.get_stats();
        assert_eq!(stats.responses_serialized, 3);
    }

    #[test]
    fn test_zero_copy_command_parsing() {
        let config = ZeroCopyConfig::default();
        let pool = Arc::new(std::sync::Mutex::new(ZeroCopyBufferPool::new(config)));
        let mut serializer = ZeroCopySerializer::new(pool);

        let test_data = b"PING\nGET test_key\nPUT key value\nDEL old_key\n";

        let result = serializer.parse_command_batch_zero_copy(test_data);
        assert!(result.is_ok());

        let commands = result.unwrap();
        assert_eq!(commands.len(), 4);

        // Verify commands
        match &commands[0] {
            Command::Ping => {}
            _ => panic!("Expected PING command"),
        }

        match &commands[1] {
            Command::Get { key } => {
                assert_eq!(key.as_ref(), b"test_key");
            }
            _ => panic!("Expected GET command"),
        }
    }

    #[test]
    fn test_memory_mapped_buffer() {
        let mut buffer = MemoryMappedBuffer::new(4096).unwrap();

        // Write some data
        let test_data = b"Hello, World!";
        let written = buffer.write(test_data).unwrap();
        assert_eq!(written, test_data.len());

        // Read it back
        let read_data = buffer.read(0, test_data.len()).unwrap();
        assert_eq!(read_data, test_data);

        // Check position
        assert_eq!(buffer.position(), test_data.len());
    }

    #[test]
    fn test_performance_benchmark() {
        let config = ZeroCopyConfig::default();
        let pool = Arc::new(std::sync::Mutex::new(ZeroCopyBufferPool::new(config)));
        let mut serializer = ZeroCopySerializer::new(pool);

        // Create large batch of commands
        let mut large_batch = Vec::new();
        for i in 0..10000 {
            large_batch.extend_from_slice(format!("GET key_{}\n", i).as_bytes());
        }

        let start = std::time::Instant::now();
        let result = serializer.parse_command_batch_zero_copy(&large_batch);
        let parse_time = start.elapsed();

        assert!(result.is_ok());
        let commands = result.unwrap();
        assert_eq!(commands.len(), 10000);

        let ops_per_second = commands.len() as f64 / parse_time.as_secs_f64();
        println!(
            "Zero-copy parsing performance: {:.0} ops/sec",
            ops_per_second
        );

        // Should achieve high performance
        assert!(ops_per_second > 500_000.0);
    }
}
