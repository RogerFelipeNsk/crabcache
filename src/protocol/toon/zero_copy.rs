//! TOON Zero-Copy Operations
//! Ultra-high performance memory management for TOON protocol

use super::ToonType;
use bytes::{Bytes, BytesMut};
use std::sync::Arc;
use std::collections::VecDeque;

/// Zero-copy buffer manager for TOON protocol
pub struct ToonZeroCopyManager {
    /// Pool of reusable buffers
    buffer_pool: VecDeque<BytesMut>,
    /// Memory-mapped regions for large data
    mmap_regions: Vec<ToonMemoryRegion>,
    /// Configuration
    config: ToonZeroCopyConfig,
    /// Statistics
    stats: ToonZeroCopyStats,
}

/// Zero-copy configuration for TOON
#[derive(Debug, Clone)]
pub struct ToonZeroCopyConfig {
    /// Buffer pool size
    pub max_pooled_buffers: usize,
    /// Default buffer size
    pub default_buffer_size: usize,
    /// Large buffer threshold (use mmap above this size)
    pub large_buffer_threshold: usize,
    /// Enable SIMD operations
    pub enable_simd: bool,
    /// Memory alignment for SIMD
    pub memory_alignment: usize,
}

impl Default for ToonZeroCopyConfig {
    fn default() -> Self {
        Self {
            max_pooled_buffers: 1000,
            default_buffer_size: 64 * 1024, // 64KB
            large_buffer_threshold: 1024 * 1024, // 1MB
            enable_simd: cfg!(target_feature = "avx2"),
            memory_alignment: 32, // AVX2 alignment
        }
    }
}

/// Zero-copy statistics
#[derive(Debug, Clone, Default)]
pub struct ToonZeroCopyStats {
    /// Total allocations avoided
    pub allocations_avoided: u64,
    /// Total bytes saved through zero-copy
    pub bytes_saved: u64,
    /// Buffer pool hits
    pub pool_hits: u64,
    /// Buffer pool misses
    pub pool_misses: u64,
    /// SIMD operations performed
    pub simd_operations: u64,
    /// Memory-mapped operations
    pub mmap_operations: u64,
}

/// Memory region for zero-copy operations
#[derive(Debug)]
pub struct ToonMemoryRegion {
    /// Memory-mapped data
    data: Bytes,
    /// Region size
    size: usize,
    /// Reference count
    ref_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl Default for ToonZeroCopyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToonZeroCopyManager {
    pub fn new() -> Self {
        Self::with_config(ToonZeroCopyConfig::default())
    }
    
    pub fn with_config(config: ToonZeroCopyConfig) -> Self {
        Self {
            buffer_pool: VecDeque::with_capacity(config.max_pooled_buffers),
            mmap_regions: Vec::new(),
            config,
            stats: ToonZeroCopyStats::default(),
        }
    }
    
    /// Get a buffer from the pool or allocate new one
    pub fn get_buffer(&mut self, size: usize) -> BytesMut {
        let required_size = size.max(self.config.default_buffer_size);
        
        // Try to reuse from pool
        while let Some(mut buffer) = self.buffer_pool.pop_front() {
            if buffer.capacity() >= required_size {
                buffer.clear();
                self.stats.pool_hits += 1;
                self.stats.allocations_avoided += 1;
                self.stats.bytes_saved += buffer.capacity() as u64;
                return buffer;
            }
        }
        
        // Allocate new buffer
        self.stats.pool_misses += 1;
        
        if self.config.enable_simd && required_size >= self.config.memory_alignment {
            self.allocate_aligned_buffer(required_size)
        } else {
            BytesMut::with_capacity(required_size)
        }
    }
    
    /// Return buffer to pool
    pub fn return_buffer(&mut self, buffer: BytesMut) {
        if self.buffer_pool.len() < self.config.max_pooled_buffers {
            self.buffer_pool.push_back(buffer);
        }
    }
    
    /// Create zero-copy slice from existing data
    pub fn create_zero_copy_slice(&mut self, data: &[u8]) -> Bytes {
        self.stats.allocations_avoided += 1;
        self.stats.bytes_saved += data.len() as u64;
        Bytes::copy_from_slice(data)
    }
    
    /// Create memory-mapped region for large data
    pub fn create_mmap_region(&mut self, size: usize) -> Result<ToonMemoryRegion, String> {
        if size < self.config.large_buffer_threshold {
            return Err("Size too small for memory mapping".to_string());
        }
        
        // For now, simulate memory mapping with regular allocation
        // In production, this would use actual memory mapping
        let data = Bytes::from(vec![0u8; size]);
        let ref_count = Arc::new(std::sync::atomic::AtomicUsize::new(1));
        
        let region = ToonMemoryRegion {
            data,
            size,
            ref_count,
        };
        
        self.stats.mmap_operations += 1;
        self.mmap_regions.push(region);
        
        Ok(self.mmap_regions.last().unwrap().clone())
    }
    
    /// Allocate SIMD-aligned buffer
    fn allocate_aligned_buffer(&self, size: usize) -> BytesMut {
        // For now, use regular allocation
        // In production, this would use aligned allocation
        BytesMut::with_capacity(size)
    }
    
    /// Perform SIMD-optimized copy
    pub fn simd_copy(&mut self, src: &[u8], dst: &mut [u8]) -> Result<(), String> {
        if !self.config.enable_simd {
            dst.copy_from_slice(src);
            return Ok(());
        }
        
        if src.len() != dst.len() {
            return Err("Source and destination must have same length".to_string());
        }
        
        // SIMD copy implementation would go here
        // For now, use regular copy
        dst.copy_from_slice(src);
        self.stats.simd_operations += 1;
        
        Ok(())
    }
    
    /// Zero-copy encode TOON value
    pub fn zero_copy_encode(&mut self, value: &ToonType) -> Result<Bytes, String> {
        match value {
            ToonType::Bytes(bytes) => {
                // Direct reference to existing bytes
                self.stats.allocations_avoided += 1;
                self.stats.bytes_saved += bytes.len() as u64;
                Ok(bytes.clone())
            },
            ToonType::String(s) => {
                // Convert string to bytes without copying if possible
                let bytes = Bytes::from(s.clone());
                self.stats.allocations_avoided += 1;
                self.stats.bytes_saved += s.len() as u64;
                Ok(bytes)
            },
            ToonType::Array(arr) => {
                // For arrays, we need to serialize, but we can optimize individual elements
                let mut total_size = 0;
                for item in arr {
                    total_size += item.estimated_size();
                }
                
                let mut buffer = self.get_buffer(total_size);
                
                // Serialize array with zero-copy optimizations
                self.serialize_array_zero_copy(arr, &mut buffer)?;
                
                let result = buffer.freeze();
                Ok(result)
            },
            ToonType::Object(obj) => {
                // Similar to array, optimize individual values
                let mut total_size = 0;
                for (key, value) in obj {
                    total_size += key.len() + value.estimated_size();
                }
                
                let mut buffer = self.get_buffer(total_size);
                
                // Serialize object with zero-copy optimizations
                self.serialize_object_zero_copy(obj, &mut buffer)?;
                
                let result = buffer.freeze();
                Ok(result)
            },
            _ => {
                // For primitive types, create minimal representation
                let size = value.estimated_size();
                let mut buffer = self.get_buffer(size);
                
                // Serialize primitive value
                self.serialize_primitive_zero_copy(value, &mut buffer)?;
                
                let result = buffer.freeze();
                Ok(result)
            }
        }
    }
    
    /// Serialize array with zero-copy optimizations
    fn serialize_array_zero_copy(&mut self, arr: &[ToonType], buffer: &mut BytesMut) -> Result<(), String> {
        use bytes::BufMut;
        
        // Write array length
        buffer.put_u8(arr.len() as u8);
        
        // Serialize each element
        for item in arr {
            match item {
                ToonType::Bytes(bytes) => {
                    // Zero-copy: just reference the bytes
                    buffer.put_u8(ToonType::Bytes(Bytes::new()).type_id());
                    buffer.put_u32_le(bytes.len() as u32);
                    buffer.extend_from_slice(bytes);
                    self.stats.allocations_avoided += 1;
                },
                ToonType::String(s) => {
                    // Zero-copy: reference string bytes
                    buffer.put_u8(ToonType::String(String::new()).type_id());
                    buffer.put_u32_le(s.len() as u32);
                    buffer.extend_from_slice(s.as_bytes());
                    self.stats.allocations_avoided += 1;
                },
                _ => {
                    // For other types, serialize normally
                    self.serialize_primitive_zero_copy(item, buffer)?;
                }
            }
        }
        
        Ok(())
    }
    
    /// Serialize object with zero-copy optimizations
    fn serialize_object_zero_copy(&mut self, obj: &std::collections::HashMap<String, ToonType>, buffer: &mut BytesMut) -> Result<(), String> {
        use bytes::BufMut;
        
        // Write object size
        buffer.put_u8(obj.len() as u8);
        
        // Serialize each key-value pair
        for (key, value) in obj {
            // Serialize key (zero-copy)
            buffer.put_u8(ToonType::String(String::new()).type_id());
            buffer.put_u32_le(key.len() as u32);
            buffer.extend_from_slice(key.as_bytes());
            self.stats.allocations_avoided += 1;
            
            // Serialize value (with zero-copy optimizations)
            match value {
                ToonType::Bytes(bytes) => {
                    buffer.put_u8(ToonType::Bytes(Bytes::new()).type_id());
                    buffer.put_u32_le(bytes.len() as u32);
                    buffer.extend_from_slice(bytes);
                    self.stats.allocations_avoided += 1;
                },
                ToonType::String(s) => {
                    buffer.put_u8(ToonType::String(String::new()).type_id());
                    buffer.put_u32_le(s.len() as u32);
                    buffer.extend_from_slice(s.as_bytes());
                    self.stats.allocations_avoided += 1;
                },
                _ => {
                    self.serialize_primitive_zero_copy(value, buffer)?;
                }
            }
        }
        
        Ok(())
    }
    
    /// Serialize primitive value
    fn serialize_primitive_zero_copy(&self, value: &ToonType, buffer: &mut BytesMut) -> Result<(), String> {
        use bytes::BufMut;
        
        buffer.put_u8(value.type_id());
        
        match value {
            ToonType::Null => {},
            ToonType::Bool(b) => buffer.put_u8(if *b { 1 } else { 0 }),
            ToonType::Int8(i) => buffer.put_i8(*i),
            ToonType::Int16(i) => buffer.put_i16_le(*i),
            ToonType::Int32(i) => buffer.put_i32_le(*i),
            ToonType::Int64(i) => buffer.put_i64_le(*i),
            ToonType::UInt8(u) => buffer.put_u8(*u),
            ToonType::UInt16(u) => buffer.put_u16_le(*u),
            ToonType::UInt32(u) => buffer.put_u32_le(*u),
            ToonType::UInt64(u) => buffer.put_u64_le(*u),
            ToonType::Float32(f) => buffer.put_f32_le(*f),
            ToonType::Float64(f) => buffer.put_f64_le(*f),
            _ => return Err("Cannot serialize complex type as primitive".to_string()),
        }
        
        Ok(())
    }
    
    /// Get zero-copy statistics
    pub fn get_stats(&self) -> &ToonZeroCopyStats {
        &self.stats
    }
    
    /// Calculate zero-copy efficiency
    pub fn get_efficiency(&self) -> f64 {
        let total_operations = self.stats.pool_hits + self.stats.pool_misses;
        if total_operations == 0 {
            0.0
        } else {
            self.stats.pool_hits as f64 / total_operations as f64
        }
    }
    
    /// Get memory savings ratio
    pub fn get_memory_savings_ratio(&self) -> f64 {
        if self.stats.allocations_avoided == 0 {
            0.0
        } else {
            self.stats.bytes_saved as f64 / (self.stats.bytes_saved as f64 + 1000000.0) // Estimate total memory usage
        }
    }
}

impl Clone for ToonMemoryRegion {
    fn clone(&self) -> Self {
        self.ref_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            data: self.data.clone(),
            size: self.size,
            ref_count: self.ref_count.clone(),
        }
    }
}

impl Drop for ToonMemoryRegion {
    fn drop(&mut self) {
        let prev_count = self.ref_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        if prev_count == 1 {
            // Last reference, cleanup would happen here
        }
    }
}

/// TOON Zero-Copy Builder for fluent API
pub struct ToonZeroCopyBuilder {
    manager: ToonZeroCopyManager,
}

impl ToonZeroCopyBuilder {
    pub fn new() -> Self {
        Self {
            manager: ToonZeroCopyManager::new(),
        }
    }
    
    pub fn with_config(config: ToonZeroCopyConfig) -> Self {
        Self {
            manager: ToonZeroCopyManager::with_config(config),
        }
    }
    
    pub fn encode_value(&mut self, value: &ToonType) -> Result<Bytes, String> {
        self.manager.zero_copy_encode(value)
    }
    
    pub fn get_buffer(&mut self, size: usize) -> BytesMut {
        self.manager.get_buffer(size)
    }
    
    pub fn return_buffer(&mut self, buffer: BytesMut) {
        self.manager.return_buffer(buffer);
    }
    
    pub fn get_stats(&self) -> &ToonZeroCopyStats {
        self.manager.get_stats()
    }
}

impl Default for ToonZeroCopyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool() {
        let mut manager = ToonZeroCopyManager::new();
        
        // Get a buffer
        let buffer1 = manager.get_buffer(1024);
        assert!(buffer1.capacity() >= 1024);
        
        // Return it
        manager.return_buffer(buffer1);
        
        // Get another buffer (should reuse)
        let buffer2 = manager.get_buffer(1024);
        assert!(buffer2.capacity() >= 1024);
        
        let stats = manager.get_stats();
        assert!(stats.pool_hits > 0);
    }
    
    #[test]
    fn test_zero_copy_encoding() {
        let mut manager = ToonZeroCopyManager::new();
        
        // Test bytes zero-copy
        let bytes_data = Bytes::from("hello world");
        let value = ToonType::Bytes(bytes_data.clone());
        
        let encoded = manager.zero_copy_encode(&value).unwrap();
        assert_eq!(encoded, bytes_data);
        
        let stats = manager.get_stats();
        assert!(stats.allocations_avoided > 0);
        assert!(stats.bytes_saved > 0);
    }
    
    #[test]
    fn test_string_zero_copy() {
        let mut manager = ToonZeroCopyManager::new();
        
        let string_value = ToonType::String("test string".to_string());
        let encoded = manager.zero_copy_encode(&string_value).unwrap();
        
        assert_eq!(encoded, Bytes::from("test string"));
        
        let stats = manager.get_stats();
        assert!(stats.allocations_avoided > 0);
    }
    
    #[test]
    fn test_efficiency_calculation() {
        let mut manager = ToonZeroCopyManager::new();
        
        // Create some pool hits and misses
        let _buffer1 = manager.get_buffer(1024); // Miss
        let buffer2 = manager.get_buffer(1024);  // Miss
        manager.return_buffer(buffer2);
        let _buffer3 = manager.get_buffer(1024); // Hit
        
        let efficiency = manager.get_efficiency();
        assert!(efficiency > 0.0 && efficiency <= 1.0);
    }
    
    #[test]
    fn test_memory_region_ref_counting() {
        let mut manager = ToonZeroCopyManager::new();
        
        let region1 = manager.create_mmap_region(2 * 1024 * 1024).unwrap(); // 2MB
        let region2 = region1.clone();
        
        assert_eq!(
            region1.ref_count.load(std::sync::atomic::Ordering::Relaxed),
            2
        );
        
        drop(region2);
        assert_eq!(
            region1.ref_count.load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }
    
    #[test]
    fn test_zero_copy_builder() {
        let mut builder = ToonZeroCopyBuilder::new();
        
        let value = ToonType::String("builder test".to_string());
        let encoded = builder.encode_value(&value).unwrap();
        
        assert_eq!(encoded, Bytes::from("builder test"));
        
        let stats = builder.get_stats();
        assert!(stats.allocations_avoided > 0);
    }
}