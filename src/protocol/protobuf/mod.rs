//! Protobuf Native Support for CrabCache
//! Phase 8.1 - Revolutionary Protobuf Integration
//!
//! This module provides native Protocol Buffers support, making CrabCache
//! the first cache system with built-in Protobuf capabilities.

pub mod buffer_pool;
pub mod generated;
pub mod negotiation;
pub mod parser;
pub mod schema_registry;
pub mod serializer;
pub mod zero_copy;

// Re-export generated protobuf types
pub use generated::*;

pub use buffer_pool::ProtobufBufferPool;
pub use negotiation::{NegotiationResult, ProtocolNegotiator, ProtocolType};
pub use parser::ProtobufParser;
pub use schema_registry::SchemaRegistry;
pub use serializer::ProtobufSerializer;
pub use zero_copy::ProtobufZeroCopy;

/// Magic bytes for Protobuf protocol detection
/// "CRAB" in ASCII: 0x43, 0x52, 0x41, 0x42
pub const PROTOBUF_MAGIC: [u8; 4] = [0x43, 0x52, 0x41, 0x42];

/// Protocol version for Protobuf support
pub const PROTOBUF_VERSION: u8 = 1;

/// Maximum message size for Protobuf (16MB)
pub const MAX_PROTOBUF_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Protobuf-specific errors
#[derive(Debug, thiserror::Error)]
pub enum ProtobufError {
    #[error("Invalid magic bytes: expected CRAB, got {0:?}")]
    InvalidMagic([u8; 4]),

    #[error("Unsupported protocol version: {0}")]
    UnsupportedVersion(u8),

    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: usize, max: usize },

    #[error("Protobuf decode error: {0}")]
    DecodeError(#[from] prost::DecodeError),

    #[error("Protobuf encode error: {0}")]
    EncodeError(#[from] prost::EncodeError),

    #[error("Schema not found: {schema_name}")]
    SchemaNotFound { schema_name: String },

    #[error("Protocol negotiation failed: {reason}")]
    NegotiationFailed { reason: String },
}

pub type ProtobufResult<T> = Result<T, ProtobufError>;

/// Protobuf protocol configuration
#[derive(Debug, Clone)]
pub struct ProtobufConfig {
    /// Enable zero-copy optimizations
    pub enable_zero_copy: bool,

    /// Enable compression for large messages
    pub enable_compression: bool,

    /// Compression threshold in bytes
    pub compression_threshold: usize,

    /// Maximum message size
    pub max_message_size: usize,

    /// Buffer pool size
    pub buffer_pool_size: usize,

    /// Enable schema caching
    pub enable_schema_cache: bool,

    /// Schema cache size
    pub schema_cache_size: usize,
}

impl Default for ProtobufConfig {
    fn default() -> Self {
        Self {
            enable_zero_copy: true,
            enable_compression: true,
            compression_threshold: 1024, // 1KB
            max_message_size: MAX_PROTOBUF_MESSAGE_SIZE,
            buffer_pool_size: 1000,
            enable_schema_cache: true,
            schema_cache_size: 100,
        }
    }
}

/// Protobuf protocol metrics
#[derive(Debug, Default, Clone)]
pub struct ProtobufMetrics {
    /// Total messages processed
    pub messages_processed: u64,

    /// Total bytes processed
    pub bytes_processed: u64,

    /// Compression ratio (compressed/original)
    pub compression_ratio: f64,

    /// Zero-copy operations percentage
    pub zero_copy_percentage: f64,

    /// Average message size
    pub avg_message_size: f64,

    /// Schema cache hit rate
    pub schema_cache_hit_rate: f64,

    /// Parse time in microseconds
    pub avg_parse_time_us: f64,

    /// Serialize time in microseconds
    pub avg_serialize_time_us: f64,
}

impl ProtobufMetrics {
    pub fn update_message_processed(
        &mut self,
        size: usize,
        parse_time_us: f64,
        serialize_time_us: f64,
    ) {
        self.messages_processed += 1;
        self.bytes_processed += size as u64;

        // Update running averages
        let count = self.messages_processed as f64;
        self.avg_message_size = (self.avg_message_size * (count - 1.0) + size as f64) / count;
        self.avg_parse_time_us = (self.avg_parse_time_us * (count - 1.0) + parse_time_us) / count;
        self.avg_serialize_time_us =
            (self.avg_serialize_time_us * (count - 1.0) + serialize_time_us) / count;
    }

    pub fn update_compression_ratio(&mut self, original_size: usize, compressed_size: usize) {
        if original_size > 0 {
            let ratio = compressed_size as f64 / original_size as f64;
            let count = self.messages_processed as f64;
            self.compression_ratio = (self.compression_ratio * (count - 1.0) + ratio) / count;
        }
    }

    pub fn update_zero_copy_percentage(&mut self, zero_copy_ops: u64, total_ops: u64) {
        if total_ops > 0 {
            self.zero_copy_percentage = (zero_copy_ops as f64 / total_ops as f64) * 100.0;
        }
    }

    pub fn update_schema_cache_hit_rate(&mut self, hits: u64, total: u64) {
        if total > 0 {
            self.schema_cache_hit_rate = (hits as f64 / total as f64) * 100.0;
        }
    }
}
