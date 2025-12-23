//! Connection pool implementation for high-performance client
//! 
//! Provides intelligent connection pooling with health checks and
//! load balancing for maximum throughput.

use super::{ClientError, ClientResult};
use crate::protocol::binary::BinaryProtocol;
use crate::protocol::commands::{Command, Response};
use bytes::Bytes;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_connections: usize,
    pub min_connections: usize,
    pub connection_timeout: Duration,
    pub health_check_interval: Duration,
    pub max_idle_time: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 20,
            min_connections: 5,
            connection_timeout: Duration::from_secs(5),
            health_check_interval: Duration::from_secs(30),
            max_idle_time: Duration::from_secs(300),
        }
    }
}

/// Pool metrics for monitoring
#[derive(Debug, Clone, Default)]
pub struct PoolMetrics {
    pub active_connections: usize,
    pub idle_connections: usize,
    pub total_created: u64,
    pub total_destroyed: u64,
    pub pool_hits: u64,
    pub pool_misses: u64,
    pub health_check_failures: u64,
}

/// A single connection in the pool
struct PoolConnection {
    stream: TcpStream,
    last_used: Instant,
    is_healthy: bool,
}

impl PoolConnection {
    fn new(stream: TcpStream) -> Self {
        let now = Instant::now();
        Self {
            stream,
            last_used: now,
            is_healthy: true,
        }
    }
    
    fn update_last_used(&mut self) {
        self.last_used = Instant::now();
    }
    
    fn is_expired(&self, max_idle_time: Duration) -> bool {
        self.last_used.elapsed() > max_idle_time
    }
}

/// High-performance connection pool
pub struct ConnectionPool {
    address: String,
    config: PoolConfig,
    connections: Arc<Mutex<VecDeque<PoolConnection>>>,
    metrics: Arc<Mutex<PoolMetrics>>,
}

impl ConnectionPool {
    /// Create a new connection pool
    pub async fn new(address: String, config: PoolConfig) -> ClientResult<Self> {
        let pool = Self {
            address,
            config,
            connections: Arc::new(Mutex::new(VecDeque::new())),
            metrics: Arc::new(Mutex::new(PoolMetrics::default())),
        };
        
        // Pre-populate with minimum connections
        pool.ensure_min_connections().await?;
        
        Ok(pool)
    }
    
    /// Get a connection from the pool
    pub async fn get_connection(&self) -> ClientResult<PooledConnection> {
        // Try to get from pool first
        if let Some(mut conn) = self.try_get_from_pool() {
            // Update metrics
            {
                let mut metrics = self.metrics.lock().unwrap();
                metrics.pool_hits += 1;
                metrics.active_connections += 1;
                metrics.idle_connections = metrics.idle_connections.saturating_sub(1);
            }
            
            conn.update_last_used();
            
            // Quick health check
            if self.quick_health_check(&mut conn.stream).await {
                return Ok(PooledConnection::new(conn, self));
            }
        }
        
        // Create new connection
        let stream = self.create_connection().await?;
        let conn = PoolConnection::new(stream);
        
        // Update metrics
        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.pool_misses += 1;
            metrics.active_connections += 1;
            metrics.total_created += 1;
        }
        
        Ok(PooledConnection::new(conn, self))
    }
    
    /// Return a connection to the pool
    async fn return_connection(&self, mut conn: PoolConnection) {
        // Update metrics
        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.active_connections = metrics.active_connections.saturating_sub(1);
        }
        
        // Check if connection is still healthy and not expired
        if conn.is_healthy && !conn.is_expired(self.config.max_idle_time) {
            let mut connections = self.connections.lock().unwrap();
            if connections.len() < self.config.max_connections {
                connections.push_back(conn);
                
                let mut metrics = self.metrics.lock().unwrap();
                metrics.idle_connections += 1;
                return;
            }
        }
        
        // Connection not returned to pool, count as destroyed
        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_destroyed += 1;
    }
    
    /// Perform health check on all connections
    pub async fn health_check(&self) {
        let mut connections_to_check = Vec::new();
        
        // Get all connections for health check
        {
            let mut connections = self.connections.lock().unwrap();
            while let Some(conn) = connections.pop_front() {
                connections_to_check.push(conn);
            }
        }
        
        let mut healthy_connections = Vec::new();
        let mut failed_count = 0;
        
        // Check each connection
        for mut conn in connections_to_check {
            if self.full_health_check(&mut conn.stream).await {
                conn.is_healthy = true;
                healthy_connections.push(conn);
            } else {
                conn.is_healthy = false;
                failed_count += 1;
            }
        }
        
        // Return healthy connections to pool
        {
            let mut connections = self.connections.lock().unwrap();
            for conn in healthy_connections {
                connections.push_back(conn);
            }
            
            let mut metrics = self.metrics.lock().unwrap();
            metrics.health_check_failures += failed_count;
            metrics.idle_connections = connections.len();
            metrics.total_destroyed += failed_count;
        }
        
        // Ensure minimum connections
        let _ = self.ensure_min_connections().await;
    }
    
    /// Get current pool metrics
    pub fn metrics(&self) -> PoolMetrics {
        self.metrics.lock().unwrap().clone()
    }
    
    // Private methods
    
    fn try_get_from_pool(&self) -> Option<PoolConnection> {
        let mut connections = self.connections.lock().unwrap();
        connections.pop_front()
    }
    
    async fn create_connection(&self) -> ClientResult<TcpStream> {
        let stream = timeout(
            self.config.connection_timeout,
            TcpStream::connect(&self.address)
        ).await
        .map_err(|_| ClientError::TimeoutError("Connection timeout".to_string()))?
        .map_err(|e| ClientError::ConnectionFailed(e.to_string()))?;
        
        // Configure TCP socket for performance
        if let Ok(socket) = stream.into_std() {
            socket.set_nodelay(true).ok(); // Disable Nagle's algorithm
            let stream = TcpStream::from_std(socket)
                .map_err(|e| ClientError::ConnectionFailed(e.to_string()))?;
            Ok(stream)
        } else {
            Err(ClientError::ConnectionFailed("Failed to configure socket".to_string()))
        }
    }
    
    async fn quick_health_check(&self, stream: &mut TcpStream) -> bool {
        // Quick PING to check if connection is alive
        let ping_cmd = BinaryProtocol::serialize_command(&Command::Ping);
        
        if stream.write_all(&ping_cmd).await.is_err() {
            return false;
        }
        
        let mut response_buf = [0u8; 1];
        if stream.read_exact(&mut response_buf).await.is_err() {
            return false;
        }
        
        response_buf[0] == 0x11 // RESP_PONG
    }
    
    async fn full_health_check(&self, stream: &mut TcpStream) -> bool {
        // More thorough health check with timeout
        let health_check = async {
            let ping_cmd = BinaryProtocol::serialize_command(&Command::Ping);
            stream.write_all(&ping_cmd).await?;
            
            let mut response_buf = [0u8; 1];
            stream.read_exact(&mut response_buf).await?;
            
            Ok::<bool, std::io::Error>(response_buf[0] == 0x11)
        };
        
        timeout(Duration::from_millis(1000), health_check)
            .await
            .unwrap_or(Ok(false))
            .unwrap_or(false)
    }
    
    async fn ensure_min_connections(&self) -> ClientResult<()> {
        let current_count = {
            let connections = self.connections.lock().unwrap();
            connections.len()
        };
        
        for _ in current_count..self.config.min_connections {
            match self.create_connection().await {
                Ok(stream) => {
                    let conn = PoolConnection::new(stream);
                    let mut connections = self.connections.lock().unwrap();
                    connections.push_back(conn);
                    
                    let mut metrics = self.metrics.lock().unwrap();
                    metrics.total_created += 1;
                    metrics.idle_connections += 1;
                }
                Err(_) => break, // Stop trying if we can't create connections
            }
        }
        
        Ok(())
    }
}

/// A connection borrowed from the pool
pub struct PooledConnection {
    connection: Option<PoolConnection>,
    pool: *const ConnectionPool,
}

impl PooledConnection {
    fn new(connection: PoolConnection, pool: &ConnectionPool) -> Self {
        Self {
            connection: Some(connection),
            pool: pool as *const ConnectionPool,
        }
    }
    
    /// Execute a command on this connection
    pub async fn execute(&mut self, command: &Command) -> ClientResult<Response> {
        let conn = self.connection.as_mut()
            .ok_or(ClientError::ConnectionFailed("Connection already returned".to_string()))?;
        
        // Serialize command using binary protocol
        let cmd_bytes = BinaryProtocol::serialize_command(command);
        
        // Send command
        conn.stream.write_all(&cmd_bytes).await
            .map_err(|e| ClientError::IoError(e))?;
        
        // Read response
        let mut response_type = [0u8; 1];
        conn.stream.read_exact(&mut response_type).await
            .map_err(|e| ClientError::IoError(e))?;
        
        match response_type[0] {
            0x10 => Ok(Response::Ok),
            0x11 => Ok(Response::Pong),
            0x12 => Ok(Response::Null),
            0x13 => {
                // Error response
                let mut len_buf = [0u8; 4];
                conn.stream.read_exact(&mut len_buf).await
                    .map_err(|e| ClientError::IoError(e))?;
                let len = u32::from_le_bytes(len_buf) as usize;
                
                let mut msg_buf = vec![0u8; len];
                conn.stream.read_exact(&mut msg_buf).await
                    .map_err(|e| ClientError::IoError(e))?;
                
                let msg = String::from_utf8_lossy(&msg_buf).to_string();
                Ok(Response::Error(msg))
            }
            0x14 => {
                // Value response
                let mut len_buf = [0u8; 4];
                conn.stream.read_exact(&mut len_buf).await
                    .map_err(|e| ClientError::IoError(e))?;
                let len = u32::from_le_bytes(len_buf) as usize;
                
                let mut value_buf = vec![0u8; len];
                conn.stream.read_exact(&mut value_buf).await
                    .map_err(|e| ClientError::IoError(e))?;
                
                Ok(Response::Value(Bytes::from(value_buf)))
            }
            0x15 => {
                // Stats response
                let mut len_buf = [0u8; 4];
                conn.stream.read_exact(&mut len_buf).await
                    .map_err(|e| ClientError::IoError(e))?;
                let len = u32::from_le_bytes(len_buf) as usize;
                
                let mut stats_buf = vec![0u8; len];
                conn.stream.read_exact(&mut stats_buf).await
                    .map_err(|e| ClientError::IoError(e))?;
                
                let stats = String::from_utf8_lossy(&stats_buf).to_string();
                Ok(Response::Stats(stats))
            }
            _ => Err(ClientError::ProtocolError(format!("Unknown response type: {}", response_type[0]))),
        }
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            // Return connection to pool
            let pool = unsafe { &*self.pool };
            let pool_clone = pool.clone();
            tokio::spawn(async move {
                pool_clone.return_connection(connection).await;
            });
        }
    }
}

// Make ConnectionPool cloneable for sharing
impl Clone for ConnectionPool {
    fn clone(&self) -> Self {
        Self {
            address: self.address.clone(),
            config: self.config.clone(),
            connections: Arc::clone(&self.connections),
            metrics: Arc::clone(&self.metrics),
        }
    }
}