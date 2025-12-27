//! Zero-Copy Protobuf Operations
//! Phase 8.1 - Advanced zero-copy optimizations for Protobuf

use crate::protocol::protobuf::{ProtobufConfig, ProtobufResult};
use bytes::{Bytes, BytesMut};

/// Zero-copy Protobuf processor
pub struct ProtobufZeroCopy {
    config: ProtobufConfig,
    buffer_pool: Vec<BytesMut>,
}

impl ProtobufZeroCopy {
    pub fn new(config: ProtobufConfig) -> Self {
        let buffer_pool_size = config.buffer_pool_size;
        Self {
            config,
            buffer_pool: Vec::with_capacity(buffer_pool_size),
        }
    }

    /// Get a buffer from the pool or create a new one
    pub fn get_buffer(&mut self, size: usize) -> BytesMut {
        if let Some(mut buf) = self.buffer_pool.pop() {
            buf.clear();
            if buf.capacity() >= size {
                return buf;
            }
        }

        BytesMut::with_capacity(size.max(4096))
    }

    /// Return a buffer to the pool
    pub fn return_buffer(&mut self, buf: BytesMut) {
        if self.buffer_pool.len() < self.config.buffer_pool_size {
            self.buffer_pool.push(buf);
        }
    }

    /// Zero-copy message parsing (placeholder for future optimization)
    pub fn parse_zero_copy(&mut self, data: Bytes) -> ProtobufResult<Bytes> {
        // For now, just return the data as-is
        // Future: Implement true zero-copy parsing with memory mapping
        Ok(data)
    }
}

impl Default for ProtobufZeroCopy {
    fn default() -> Self {
        Self::new(ProtobufConfig::default())
    }
}
