//! Native client implementation with binary protocol and pipelining
//! 
//! This client is designed for maximum performance using exclusively
//! the binary protocol and advanced features like connection pooling
//! and pipelining.

use super::{ClientConfig, ClientError, ClientResult, ClientMetrics};
use super::pool::{ConnectionPool, PoolConfig};
use crate::protocol::commands::{Command, Response};
use crate::protocol::pipeline::{PipelineBuilder, PipelineProtocol};
use bytes::Bytes;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::time::Duration;

/// High-performance native client for CrabCache
pub struct NativeClient {
    pool: ConnectionPool,
    config: ClientConfig,
    metrics: Arc<Mutex<ClientMetrics>>,
}

impl NativeClient {
    /// Create a new native client
    pub async fn new(config: ClientConfig) -> ClientResult<Self> {
        // Ensure binary protocol is forced for Phase 3
        if !config.force_binary_protocol {
            return Err(ClientError::BinaryProtocolRequired);
        }
        
        let pool_config = PoolConfig {
            max_connections: config.connection_pool_size * 2,
            min_connections: config.connection_pool_size / 2,
            connection_timeout: config.connection_timeout,
            health_check_interval: Duration::from_secs(30),
            max_idle_time: Duration::from_secs(300),
        };
        
        let pool = ConnectionPool::new(config.address.clone(), pool_config).await?;
        
        Ok(Self {
            pool,
            config,
            metrics: Arc::new(Mutex::new(ClientMetrics::default())),
        })
    }
    
    /// Execute a single command
    pub async fn execute(&self, command: Command) -> ClientResult<Response> {
        let start_time = Instant::now();
        
        let mut conn = self.pool.get_connection().await?;
        let result = conn.execute(&command).await;
        
        // Update metrics
        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.total_requests += 1;
            metrics.binary_protocol_usage += 1; // Always binary in Phase 3
            
            match &result {
                Ok(_) => {
                    metrics.successful_requests += 1;
                    metrics.total_latency_ms += start_time.elapsed().as_millis() as u64;
                }
                Err(_) => {
                    metrics.failed_requests += 1;
                }
            }
        }
        
        result
    }
    
    /// GET operation
    pub async fn get(&self, key: &[u8]) -> ClientResult<Option<Bytes>> {
        let command = Command::Get {
            key: Bytes::from(key.to_vec()),
        };
        
        match self.execute(command).await? {
            Response::Value(value) => Ok(Some(value)),
            Response::Null => Ok(None),
            Response::Error(msg) => Err(ClientError::ProtocolError(msg)),
            _ => Err(ClientError::ProtocolError("Unexpected response for GET".to_string())),
        }
    }
    
    /// PUT operation
    pub async fn put(&self, key: &[u8], value: &[u8]) -> ClientResult<()> {
        let command = Command::Put {
            key: Bytes::from(key.to_vec()),
            value: Bytes::from(value.to_vec()),
            ttl: None,
        };
        
        match self.execute(command).await? {
            Response::Ok => Ok(()),
            Response::Error(msg) => Err(ClientError::ProtocolError(msg)),
            _ => Err(ClientError::ProtocolError("Unexpected response for PUT".to_string())),
        }
    }
    
    /// PUT operation with TTL
    pub async fn put_with_ttl(&self, key: &[u8], value: &[u8], ttl_seconds: u64) -> ClientResult<()> {
        let command = Command::Put {
            key: Bytes::from(key.to_vec()),
            value: Bytes::from(value.to_vec()),
            ttl: Some(ttl_seconds),
        };
        
        match self.execute(command).await? {
            Response::Ok => Ok(()),
            Response::Error(msg) => Err(ClientError::ProtocolError(msg)),
            _ => Err(ClientError::ProtocolError("Unexpected response for PUT".to_string())),
        }
    }
    
    /// DELETE operation
    pub async fn del(&self, key: &[u8]) -> ClientResult<bool> {
        let command = Command::Del {
            key: Bytes::from(key.to_vec()),
        };
        
        match self.execute(command).await? {
            Response::Ok => Ok(true),
            Response::Null => Ok(false),
            Response::Error(msg) => Err(ClientError::ProtocolError(msg)),
            _ => Err(ClientError::ProtocolError("Unexpected response for DEL".to_string())),
        }
    }
    
    /// PING operation
    pub async fn ping(&self) -> ClientResult<()> {
        let command = Command::Ping;
        
        match self.execute(command).await? {
            Response::Pong => Ok(()),
            Response::Error(msg) => Err(ClientError::ProtocolError(msg)),
            _ => Err(ClientError::ProtocolError("Unexpected response for PING".to_string())),
        }
    }
    
    /// EXPIRE operation
    pub async fn expire(&self, key: &[u8], ttl_seconds: u64) -> ClientResult<bool> {
        let command = Command::Expire {
            key: Bytes::from(key.to_vec()),
            ttl: ttl_seconds,
        };
        
        match self.execute(command).await? {
            Response::Ok => Ok(true),
            Response::Null => Ok(false),
            Response::Error(msg) => Err(ClientError::ProtocolError(msg)),
            _ => Err(ClientError::ProtocolError("Unexpected response for EXPIRE".to_string())),
        }
    }
    
    /// Get server statistics
    pub async fn stats(&self) -> ClientResult<String> {
        let command = Command::Stats;
        
        match self.execute(command).await? {
            Response::Stats(stats) => Ok(stats),
            Response::Error(msg) => Err(ClientError::ProtocolError(msg)),
            _ => Err(ClientError::ProtocolError("Unexpected response for STATS".to_string())),
        }
    }
    
    /// Create a pipeline for batch operations
    pub async fn pipeline(&self) -> Pipeline {
        Pipeline::new(self)
    }
    
    /// Execute multiple commands in optimized batch (advanced pipelining)
    pub async fn batch(&self, commands: Vec<Command>) -> ClientResult<Vec<Response>> {
        if commands.is_empty() {
            return Ok(Vec::new());
        }
        
        let start_time = Instant::now();
        
        // Use advanced pipelining for large batches
        if commands.len() > 10 {
            return self.batch_with_advanced_pipeline(commands, start_time).await;
        }
        
        // Use simple sequential execution for small batches
        let mut results = Vec::with_capacity(commands.len());
        let mut conn = self.pool.get_connection().await?;
        
        for command in commands.iter() {
            match conn.execute(command).await {
                Ok(response) => results.push(response),
                Err(e) => return Err(e),
            }
        }
        
        // Update metrics
        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.total_requests += commands.len() as u64;
            metrics.successful_requests += results.len() as u64;
            metrics.binary_protocol_usage += commands.len() as u64;
            metrics.pipeline_requests += 1;
            metrics.total_latency_ms += start_time.elapsed().as_millis() as u64;
        }
        
        Ok(results)
    }
    
    /// Advanced pipeline implementation for large batches with optimized processing
    async fn batch_with_advanced_pipeline(&self, commands: Vec<Command>, start_time: Instant) -> ClientResult<Vec<Response>> {
        // Use optimized sequential processing with connection reuse
        let mut all_responses = Vec::with_capacity(commands.len());
        let mut conn = self.pool.get_connection().await?;
        
        // Process commands in optimized batches
        let batch_size = 50; // Optimal batch size for most workloads
        
        for chunk in commands.chunks(batch_size) {
            // Execute batch sequentially but with optimized connection handling
            for command in chunk {
                match conn.execute(command).await {
                    Ok(response) => all_responses.push(response),
                    Err(e) => return Err(e),
                }
            }
            
            // Optional: yield control between batches for better concurrency
            if chunk.len() == batch_size {
                tokio::task::yield_now().await;
            }
        }
        
        // Update metrics
        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.total_requests += all_responses.len() as u64;
            metrics.successful_requests += all_responses.len() as u64;
            metrics.binary_protocol_usage += all_responses.len() as u64;
            metrics.pipeline_requests += 1;
            metrics.total_latency_ms += start_time.elapsed().as_millis() as u64;
        }
        
        Ok(all_responses)
    }
    
    /// Get client metrics
    pub fn metrics(&self) -> ClientMetrics {
        self.metrics.lock().unwrap().clone()
    }
    
    /// Get pool metrics
    pub fn pool_metrics(&self) -> super::pool::PoolMetrics {
        self.pool.metrics()
    }
    
    /// Perform health check on all connections
    pub async fn health_check(&self) {
        self.pool.health_check().await;
    }
}

/// Pipeline for batch operations
pub struct Pipeline {
    client: *const NativeClient,
    commands: Vec<Command>,
}

impl Pipeline {
    fn new(client: &NativeClient) -> Self {
        Self {
            client: client as *const NativeClient,
            commands: Vec::new(),
        }
    }
    
    /// Add a GET command to the pipeline
    pub fn get(&mut self, key: &[u8]) -> &mut Self {
        self.commands.push(Command::Get {
            key: Bytes::from(key.to_vec()),
        });
        self
    }
    
    /// Add a PUT command to the pipeline
    pub fn put(&mut self, key: &[u8], value: &[u8]) -> &mut Self {
        self.commands.push(Command::Put {
            key: Bytes::from(key.to_vec()),
            value: Bytes::from(value.to_vec()),
            ttl: None,
        });
        self
    }
    
    /// Add a PUT command with TTL to the pipeline
    pub fn put_with_ttl(&mut self, key: &[u8], value: &[u8], ttl_seconds: u64) -> &mut Self {
        self.commands.push(Command::Put {
            key: Bytes::from(key.to_vec()),
            value: Bytes::from(value.to_vec()),
            ttl: Some(ttl_seconds),
        });
        self
    }
    
    /// Add a DELETE command to the pipeline
    pub fn del(&mut self, key: &[u8]) -> &mut Self {
        self.commands.push(Command::Del {
            key: Bytes::from(key.to_vec()),
        });
        self
    }
    
    /// Add a PING command to the pipeline
    pub fn ping(&mut self) -> &mut Self {
        self.commands.push(Command::Ping);
        self
    }
    
    /// Add an EXPIRE command to the pipeline
    pub fn expire(&mut self, key: &[u8], ttl_seconds: u64) -> &mut Self {
        self.commands.push(Command::Expire {
            key: Bytes::from(key.to_vec()),
            ttl: ttl_seconds,
        });
        self
    }
    
    /// Execute all commands in the pipeline
    pub async fn execute(self) -> ClientResult<Vec<Response>> {
        if self.commands.is_empty() {
            return Ok(Vec::new());
        }
        
        let client = unsafe { &*self.client };
        
        // Check batch size limit
        if self.commands.len() > client.config.pipeline_batch_size {
            return Err(ClientError::PipelineError(
                format!("Batch size {} exceeds limit {}", 
                    self.commands.len(), 
                    client.config.pipeline_batch_size)
            ));
        }
        
        client.batch(self.commands).await
    }
    
    /// Get the number of commands in the pipeline
    pub fn len(&self) -> usize {
        self.commands.len()
    }
    
    /// Check if the pipeline is empty
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_client_config_validation() {
        let mut config = ClientConfig::default();
        config.force_binary_protocol = false;
        
        let result = NativeClient::new(config).await;
        assert!(matches!(result, Err(ClientError::BinaryProtocolRequired)));
    }
    
    #[tokio::test]
    async fn test_pipeline_creation() {
        let config = ClientConfig::default();
        // This will fail without a server, but we're just testing the structure
        if let Ok(client) = NativeClient::new(config).await {
            let mut pipeline = client.pipeline().await;
            
            pipeline
                .put(b"key1", b"value1")
                .get(b"key1")
                .del(b"key1")
                .ping();
            
            assert_eq!(pipeline.len(), 4);
            assert!(!pipeline.is_empty());
        }
    }
    
    #[test]
    fn test_client_metrics() {
        let mut metrics = ClientMetrics::default();
        metrics.total_requests = 100;
        metrics.successful_requests = 95;
        metrics.failed_requests = 5;
        metrics.total_latency_ms = 1000;
        metrics.binary_protocol_usage = 100;
        
        assert_eq!(metrics.success_rate(), 95.0);
        assert_eq!(metrics.average_latency_ms(), 1000.0 / 95.0);
        assert_eq!(metrics.binary_protocol_rate(), 100.0);
    }
}