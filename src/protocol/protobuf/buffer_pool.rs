//! Buffer Pool for Protobuf Operations
//! Phase 8.1 - Efficient buffer management for high-performance parsing

use bytes::BytesMut;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Thread-safe buffer pool for Protobuf operations
pub struct ProtobufBufferPool {
    buffers: Arc<Mutex<VecDeque<BytesMut>>>,
    max_size: usize,
    default_capacity: usize,
}

impl ProtobufBufferPool {
    pub fn new(max_size: usize, default_capacity: usize) -> Self {
        Self {
            buffers: Arc::new(Mutex::new(VecDeque::with_capacity(max_size))),
            max_size,
            default_capacity,
        }
    }

    /// Get a buffer from the pool
    pub fn get_buffer(&self) -> BytesMut {
        if let Ok(mut buffers) = self.buffers.lock() {
            if let Some(mut buf) = buffers.pop_front() {
                buf.clear();
                return buf;
            }
        }

        // Create new buffer if pool is empty
        BytesMut::with_capacity(self.default_capacity)
    }

    /// Return a buffer to the pool
    pub fn return_buffer(&self, buf: BytesMut) {
        if let Ok(mut buffers) = self.buffers.lock() {
            if buffers.len() < self.max_size {
                buffers.push_back(buf);
            }
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        if let Ok(buffers) = self.buffers.lock() {
            PoolStats {
                available_buffers: buffers.len(),
                max_size: self.max_size,
                default_capacity: self.default_capacity,
            }
        } else {
            PoolStats {
                available_buffers: 0,
                max_size: self.max_size,
                default_capacity: self.default_capacity,
            }
        }
    }
}

impl Default for ProtobufBufferPool {
    fn default() -> Self {
        Self::new(1000, 4096)
    }
}

/// Buffer pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub available_buffers: usize,
    pub max_size: usize,
    pub default_capacity: usize,
}
