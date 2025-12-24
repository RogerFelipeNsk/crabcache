//! Native client implementation for CrabCache
//!
//! This module provides a high-performance native client that uses
//! exclusively the binary protocol for maximum performance.

pub mod native;
pub mod pool;

pub use native::NativeClient;
pub use pool::{ConnectionPool, PoolConfig, PooledConnection};
use std::time::Duration;
use thiserror::Error;

/// Client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Server address to connect to
    pub address: String,
    /// Connection pool size
    pub connection_pool_size: usize,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Pipeline batch size
    pub pipeline_batch_size: usize,
    /// Enable pipelining
    pub enable_pipelining: bool,
    /// Force binary protocol (Phase 3 requirement)
    pub force_binary_protocol: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:7001".to_string(),
            connection_pool_size: 10,
            connection_timeout: Duration::from_secs(5),
            pipeline_batch_size: 100,
            enable_pipelining: true,
            force_binary_protocol: true, // Phase 3: Force binary protocol
        }
    }
}

/// Client metrics for monitoring
#[derive(Debug, Clone, Default)]
pub struct ClientMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_latency_ms: u64,
    pub pipeline_requests: u64,
    pub binary_protocol_usage: u64,
}

impl ClientMetrics {
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.successful_requests as f64 / self.total_requests as f64 * 100.0
        }
    }

    pub fn average_latency_ms(&self) -> f64 {
        if self.successful_requests == 0 {
            0.0
        } else {
            self.total_latency_ms as f64 / self.successful_requests as f64
        }
    }

    pub fn binary_protocol_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.binary_protocol_usage as f64 / self.total_requests as f64 * 100.0
        }
    }
}

/// Client errors
#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("Pool exhausted")]
    PoolExhausted,

    #[error("Binary protocol required but not supported")]
    BinaryProtocolRequired,

    #[error("Pipeline error: {0}")]
    PipelineError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type ClientResult<T> = Result<T, ClientError>;
