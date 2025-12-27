//! TCP server implementation

use crate::metrics::SharedMetrics;
use crate::protocol::commands::Response;
use crate::protocol::{BinaryProtocol, PipelineProcessor, ProtocolParser, ProtocolSerializer};
use crate::router::ShardRouter;
use crate::security::{
    AuthManager, IpFilter, RateLimiter, SecurityCheckResult, SecurityContext, SecurityManager,
};
use crate::server::MetricsServer;
use crate::shard::eviction_manager::EvictionShardManager;
use crate::shard::optimized_manager::OptimizedShardManager;
use crate::shard::WALShardManager;
use crate::Config;
use bytes::BytesMut;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

mod buffer_pool;
mod connection_metrics;
mod streaming_buffer;

use buffer_pool::BufferPool;
use connection_metrics::{CommandTimer, ConnectionGuard, ConnectionMetrics};
use streaming_buffer::StreamingBuffer;

/// Enum para diferentes tipos de shard manager
pub enum ShardManagerType {
    Optimized(Arc<OptimizedShardManager>),
    Eviction(Arc<EvictionShardManager>),
    WAL(Arc<WALShardManager>),
}

impl ShardManagerType {
    pub async fn process_command(&self, command: crate::protocol::commands::Command) -> Response {
        match self {
            ShardManagerType::Optimized(manager) => {
                manager.process_command_optimized(command).await
            }
            ShardManagerType::Eviction(manager) => manager.process_command(command).await,
            ShardManagerType::WAL(manager) => manager.process_command(command).await,
        }
    }

    /// Process batch of commands for pipelining
    pub async fn process_batch(
        &self,
        commands: Vec<crate::protocol::commands::Command>,
    ) -> Vec<Response> {
        let mut responses = Vec::with_capacity(commands.len());

        for command in commands {
            let response = self.process_command(command).await;
            responses.push(response);
        }

        responses
    }
}

/// TCP server for CrabCache with extreme performance optimizations
pub struct TcpServer {
    config: Arc<Config>,
    router: Arc<ShardRouter>,
    shard_manager: ShardManagerType,
    buffer_pool: Arc<BufferPool>,
    metrics: Arc<ConnectionMetrics>,
    shared_metrics: SharedMetrics,
    security_manager: Arc<SecurityManager>,
    pipeline_enabled: bool,
    max_pipeline_batch_size: usize,
}

impl TcpServer {
    /// Create a new TCP server with extreme performance optimizations
    pub async fn new(config: Config) -> crate::Result<Self> {
        let num_shards = config.get_num_shards();
        let max_memory_per_shard = config.max_memory_per_shard;
        let router = Arc::new(ShardRouter::new(num_shards, max_memory_per_shard));

        // Choose shard manager based on configuration
        let shard_manager = if config.enable_wal {
            info!("Initializing with WAL-enabled shard manager");

            // Create WAL config
            let wal_config = match config.get_wal_config() {
                Ok(wal_config) => Some(wal_config),
                Err(e) => {
                    warn!("Invalid WAL config, disabling WAL: {}", e);
                    None
                }
            };

            // Create eviction config
            let eviction_config = config.eviction.clone();

            // Try to create WAL manager, fallback to eviction if it fails
            match WALShardManager::new_with_recovery(
                num_shards,
                max_memory_per_shard,
                eviction_config,
                wal_config,
            )
            .await
            {
                Ok((wal_manager, recovery_stats)) => {
                    if let Some(stats) = recovery_stats {
                        info!(
                            "WAL recovery completed: {} entries recovered in {}ms",
                            stats.entries_recovered, stats.recovery_time_ms
                        );
                    }
                    ShardManagerType::WAL(Arc::new(wal_manager))
                }
                Err(e) => {
                    error!(
                        "Failed to initialize WAL manager, falling back to eviction: {}",
                        e
                    );
                    let eviction_manager = EvictionShardManager::new(
                        num_shards,
                        max_memory_per_shard,
                        config.eviction.clone(),
                    )
                    .map_err(|e| format!("Failed to create eviction manager: {}", e))?;
                    ShardManagerType::Eviction(Arc::new(eviction_manager))
                }
            }
        } else if config.eviction.enabled {
            info!("Initializing with eviction-enabled shard manager (TinyLFU)");
            let eviction_manager = EvictionShardManager::new(
                num_shards,
                max_memory_per_shard,
                config.eviction.clone(),
            )
            .map_err(|e| format!("Failed to create eviction manager: {}", e))?;
            ShardManagerType::Eviction(Arc::new(eviction_manager))
        } else {
            info!("Initializing with optimized shard manager (no eviction)");
            let optimized_manager = OptimizedShardManager::new(num_shards, max_memory_per_shard);
            ShardManagerType::Optimized(Arc::new(optimized_manager))
        };

        // Create buffer pool for reducing allocations - INCREASED SIZE for binary data
        let buffer_pool = Arc::new(BufferPool::new(
            65536, // 64KB buffers (increased from 16KB for large binary data)
            100,   // Keep up to 100 buffers in pool
        ));

        // Create metrics for monitoring performance
        let metrics = Arc::new(ConnectionMetrics::new());

        // Get shared metrics based on manager type
        let shared_metrics = match &shard_manager {
            ShardManagerType::Optimized(manager) => manager.get_shared_metrics(),
            ShardManagerType::Eviction(_) => {
                // For eviction manager, create default shared metrics
                crate::metrics::create_shared_metrics(num_shards)
            }
            ShardManagerType::WAL(_) => {
                // For WAL manager, create default shared metrics
                crate::metrics::create_shared_metrics(num_shards)
            }
        };

        // Create security manager
        let security_manager = Arc::new(Self::create_security_manager(&config)?);

        // Pipeline configuration
        let pipeline_enabled = config.connection.pipeline.enabled;
        let max_pipeline_batch_size = config.connection.pipeline.max_batch_size;

        info!("TCP Server initialized with EXTREME optimizations:");
        info!(
            "  - {} shards, {}B per shard",
            num_shards, max_memory_per_shard
        );
        match &shard_manager {
            ShardManagerType::Optimized(_) => {
                info!("  - OptimizedShardManager with SIMD, lock-free, zero-copy");
            }
            ShardManagerType::Eviction(_) => {
                info!("  - EvictionShardManager with TinyLFU eviction system");
            }
            ShardManagerType::WAL(_) => {
                info!("  - WALShardManager with TinyLFU eviction and persistence");
            }
        }
        info!("  - Buffer pool: 16KB buffers, max 100 pooled");
        info!("  - Integrated metrics system with Prometheus export");
        info!(
            "  - Security: auth={}, rate_limit={}, ip_filter={}",
            config.security.enable_auth,
            config.rate_limiting.enabled,
            !config.security.allowed_ips.is_empty()
        );
        info!(
            "  - Pipelining: enabled={}, max_batch_size={}",
            pipeline_enabled, max_pipeline_batch_size
        );

        Ok(Self {
            config: Arc::new(config),
            router,
            shard_manager,
            buffer_pool,
            metrics,
            shared_metrics,
            security_manager,
            pipeline_enabled,
            max_pipeline_batch_size,
        })
    }

    /// Create security manager based on configuration
    fn create_security_manager(
        config: &Config,
    ) -> Result<SecurityManager, Box<dyn std::error::Error + Send + Sync>> {
        // Create authentication manager
        let auth_manager = if config.security.enable_auth {
            if let Some(ref token) = config.security.auth_token {
                Some(AuthManager::with_token(
                    token.clone(),
                    "default_user".to_string(),
                ))
            } else {
                warn!("Authentication enabled but no token provided");
                None
            }
        } else {
            None
        };

        // Create rate limiter
        let rate_limiter = if config.rate_limiting.enabled {
            Some(RateLimiter::new(
                config.rate_limiting.max_requests_per_second,
                config.rate_limiting.burst_capacity,
            ))
        } else {
            None
        };

        // Create IP filter
        let ip_filter = if !config.security.allowed_ips.is_empty() {
            Some(IpFilter::new(config.security.allowed_ips.clone(), false)?)
        } else {
            None
        };

        Ok(SecurityManager::new(auth_manager, rate_limiter, ip_filter))
    }

    /// Get shared metrics for external use
    pub fn get_shared_metrics(&self) -> SharedMetrics {
        Arc::clone(&self.shared_metrics)
    }

    /// Start metrics server alongside main TCP server
    pub async fn start_with_metrics(&self, metrics_port: u16) -> crate::Result<()> {
        let metrics_server = MetricsServer::new(Arc::clone(&self.shared_metrics), metrics_port);

        // Start metrics server in background
        let metrics_handle = tokio::spawn(async move {
            if let Err(e) = metrics_server.start().await {
                error!("Metrics server error: {}", e);
            }
        });

        info!("Started metrics server on port {}", metrics_port);
        info!("Available endpoints:");
        info!("  - http://localhost:{}/metrics (Prometheus)", metrics_port);
        info!("  - http://localhost:{}/dashboard (Web UI)", metrics_port);
        info!(
            "  - http://localhost:{}/health (Health check)",
            metrics_port
        );

        // Start main TCP server
        let main_result = self.start().await;

        // If main server stops, stop metrics server too
        metrics_handle.abort();

        main_result
    }

    /// Start the server with optimizations
    pub async fn start(&self) -> crate::Result<()> {
        let addr = format!("{}:{}", self.config.bind_addr, self.config.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("CrabCache TCP server listening on {}", addr);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("New connection from {}", addr);
                    let config = Arc::clone(&self.config);
                    let router = Arc::clone(&self.router);
                    let shard_manager = match &self.shard_manager {
                        ShardManagerType::Optimized(manager) => {
                            ShardManagerType::Optimized(Arc::clone(manager))
                        }
                        ShardManagerType::Eviction(manager) => {
                            ShardManagerType::Eviction(Arc::clone(manager))
                        }
                        ShardManagerType::WAL(manager) => {
                            ShardManagerType::WAL(Arc::clone(manager))
                        }
                    };
                    let buffer_pool = Arc::clone(&self.buffer_pool);
                    let metrics = Arc::clone(&self.metrics);
                    let security_manager = Arc::clone(&self.security_manager);
                    let pipeline_enabled = self.pipeline_enabled;
                    let max_pipeline_batch_size = self.max_pipeline_batch_size;

                    let shared_metrics = Arc::clone(&self.shared_metrics);

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection_extreme_optimized(
                            stream,
                            config,
                            router,
                            shard_manager,
                            buffer_pool,
                            metrics,
                            shared_metrics,
                            security_manager,
                            addr,
                            pipeline_enabled,
                            max_pipeline_batch_size,
                        )
                        .await
                        {
                            error!("Error handling connection from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    self.metrics.record_network_error();
                }
            }
        }
    }

    /// Handle a single client connection with EXTREME optimizations
    async fn handle_connection_extreme_optimized(
        mut stream: TcpStream,
        _config: Arc<Config>,
        _router: Arc<ShardRouter>,
        shard_manager: ShardManagerType,
        buffer_pool: Arc<BufferPool>,
        metrics: Arc<ConnectionMetrics>,
        shared_metrics: SharedMetrics,
        security_manager: Arc<SecurityManager>,
        client_addr: SocketAddr,
        pipeline_enabled: bool,
        max_pipeline_batch_size: usize,
    ) -> crate::Result<()> {
        // PERFORMANCE OPTIMIZATION 1: Disable Nagle's algorithm for low latency
        if let Err(e) = stream.set_nodelay(true) {
            warn!("Failed to set TCP_NODELAY: {}", e);
        }

        // Track connection lifetime
        let _connection_guard = ConnectionGuard::new(Arc::clone(&metrics));

        // Record connection in shared metrics
        if let Ok(shared_metrics_guard) = shared_metrics.try_write() {
            shared_metrics_guard.increment_connections();
        }

        // Create security context
        let mut security_context = SecurityContext::new(client_addr.ip());

        // Check initial connection security (IP filter, rate limiting)
        match security_manager.check_connection(&security_context).await {
            SecurityCheckResult::Allowed => {}
            result => {
                warn!(
                    "Connection from {} blocked: {}",
                    client_addr,
                    result.error_message()
                );
                let error_response = Response::Error(result.error_message().to_string());
                let response_bytes = match ProtocolSerializer::serialize_response(&error_response) {
                    Ok(bytes) => bytes,
                    Err(_) => return Ok(()),
                };
                let _ = stream.write_all(&response_bytes).await;
                return Ok(());
            }
        }

        // PERFORMANCE OPTIMIZATION 2: Larger response buffer (16KB vs 4KB)
        let mut response_buffer = BytesMut::with_capacity(16384);

        // Create pipeline processor if enabled
        let _pipeline_processor = if pipeline_enabled {
            Some(PipelineProcessor::new(max_pipeline_batch_size))
        } else {
            None
        };

        debug!(
            "Connection established with pipelining: enabled={}",
            pipeline_enabled
        );

        // IMPROVED: Use streaming buffer for large commands
        let mut streaming_buffer = StreamingBuffer::new(1024 * 1024); // 1MB max command size

        loop {
            // Get buffer from pool (reduces allocations)
            let mut buffer = buffer_pool.get_buffer().await;
            metrics.record_buffer_pool_hit();

            // Read data from client with timeout
            let n = match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                stream.read(&mut buffer),
            )
            .await
            {
                Ok(Ok(0)) => {
                    debug!("Client disconnected");
                    buffer_pool.return_buffer(buffer).await;
                    break;
                }
                Ok(Ok(n)) => n,
                Ok(Err(e)) => {
                    error!("Failed to read from socket: {}", e);
                    metrics.record_network_error();
                    buffer_pool.return_buffer(buffer).await;
                    break;
                }
                Err(_) => {
                    warn!("Client read timeout");
                    metrics.record_network_error();
                    buffer_pool.return_buffer(buffer).await;
                    break;
                }
            };

            // Append new data to streaming buffer
            if let Err(e) = streaming_buffer.append(&buffer[..n]) {
                error!("Command too large from {}: {}", client_addr, e);
                let error_response = Response::Error(format!("Command too large: {}", e));
                let response_bytes = match ProtocolSerializer::serialize_response(&error_response) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        buffer_pool.return_buffer(buffer).await;
                        break;
                    }
                };
                if let Err(e) = stream.write_all(&response_bytes).await {
                    error!("Failed to write error response: {}", e);
                }
                streaming_buffer.clear();
                buffer_pool.return_buffer(buffer).await;
                continue;
            }

            buffer_pool.return_buffer(buffer).await;

            // Process all complete commands in the streaming buffer
            while streaming_buffer.has_complete_commands() {
                if let Some(command_data) = streaming_buffer.extract_command() {
                    // Start timing command processing
                    let _timer = CommandTimer::new(Arc::clone(&metrics));

                    // Process single command with improved error handling
                    if let Err(e) = Self::process_single_command_improved(
                        &command_data,
                        &shard_manager,
                        &mut security_context,
                        &security_manager,
                        &mut stream,
                        &mut response_buffer,
                        &metrics,
                        client_addr,
                    )
                    .await
                    {
                        error!("Command processing failed: {}", e);
                        // Don't break connection on single command failure
                        // Send error response and continue
                        let error_response = Response::Error(format!("Processing error: {}", e));
                        let response_bytes =
                            match ProtocolSerializer::serialize_response(&error_response) {
                                Ok(bytes) => bytes,
                                Err(_) => break,
                            };
                        if let Err(e) = stream.write_all(&response_bytes).await {
                            error!("Failed to write error response: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Process single command with improved error handling and large data support
    async fn process_single_command_improved(
        data: &[u8],
        shard_manager: &ShardManagerType,
        security_context: &mut SecurityContext,
        security_manager: &Arc<SecurityManager>,
        stream: &mut TcpStream,
        response_buffer: &mut BytesMut,
        metrics: &Arc<ConnectionMetrics>,
        client_addr: SocketAddr,
    ) -> crate::Result<()> {
        // Parse single command with auto-detection and improved error handling
        let (command, protocol_type) = match Self::parse_command_auto_detect(data) {
            Ok(result) => result,
            Err(e) => {
                warn!("Failed to parse command from {}: {}", client_addr, e);
                metrics.record_processing_error();

                // Send detailed error response instead of generic parse error
                let error_response = Response::Error(format!("PARSE_ERROR: {}", e));
                let response_bytes = if data.len() >= 4 && &data[0..4] == b"TOON" {
                    ProtocolSerializer::serialize_response_toon(&error_response)?
                } else if data.len() > 0 && data[0] >= 0x01 && data[0] <= 0x06 {
                    BinaryProtocol::serialize_response(&error_response)
                } else {
                    ProtocolSerializer::serialize_response(&error_response)?
                };
                stream.write_all(&response_bytes).await?;
                return Ok(()); // Don't break connection, just return error
            }
        };

        // Security check for single command
        let auth_result = security_manager.authenticate_command(security_context, None);
        if !auth_result.is_allowed() {
            warn!(
                "Command from {} blocked: {}",
                client_addr,
                auth_result.error_message()
            );
            metrics.record_processing_error();

            let error_response =
                Response::Error(format!("AUTH_ERROR: {}", auth_result.error_message()));
            let response_bytes = match protocol_type {
                2 => ProtocolSerializer::serialize_response_toon(&error_response)?,
                1 => BinaryProtocol::serialize_response(&error_response),
                _ => ProtocolSerializer::serialize_response(&error_response)?,
            };
            stream.write_all(&response_bytes).await?;
            return Ok(());
        }

        // Process command
        let response = shard_manager.process_command(command).await;

        // Serialize and send response based on detected protocol
        response_buffer.clear();
        let response_bytes = match protocol_type {
            2 => {
                debug!("Using TOON protocol for response to {}", client_addr);
                ProtocolSerializer::serialize_response_toon(&response)?
            }
            1 => {
                debug!("Using binary protocol for response to {}", client_addr);
                BinaryProtocol::serialize_response(&response)
            }
            _ => {
                debug!("Using text protocol for response to {}", client_addr);
                ProtocolSerializer::serialize_response(&response)?
            }
        };

        stream.write_all(&response_bytes).await?;

        debug!(
            "Processed single command from {} (protocol: {})",
            client_addr,
            match protocol_type {
                2 => "TOON",
                1 => "binary",
                _ => "text",
            }
        );

        Ok(())
    }

    /// Auto-detect protocol type and parse command accordingly
    /// Returns (Command, protocol_type) where protocol_type: 0=text, 1=binary, 2=toon
    fn parse_command_auto_detect(data: &[u8]) -> crate::Result<(crate::protocol::Command, u8)> {
        if data.is_empty() {
            return Err("Empty command".into());
        }

        // Check for TOON protocol first (highest priority)
        if data.len() >= 4 && &data[0..4] == b"TOON" {
            debug!("Detected TOON protocol");
            match ProtocolParser::parse_command(data) {
                Ok(command) => return Ok((command, 2)), // TOON protocol
                Err(e) => {
                    debug!("TOON parsing failed: {}, falling back", e);
                }
            }
        }

        // Check for Protobuf protocol second
        if data.len() >= 4 && &data[0..4] == b"CRAB" {
            debug!("Detected Protobuf protocol");
            match ProtocolParser::parse_command(data) {
                Ok(command) => return Ok((command, 1)), // Binary/Protobuf protocol
                Err(e) => {
                    debug!("Protobuf parsing failed: {}, falling back", e);
                }
            }
        }

        // Binary protocol detection: first byte should be a valid command type (0x01-0x06)
        let first_byte = data[0];
        if first_byte >= 0x01 && first_byte <= 0x06 {
            // Likely binary protocol - try parsing as binary first
            match BinaryProtocol::parse_command(data) {
                Ok(command) => {
                    debug!("Using binary protocol for command");
                    return Ok((command, 1)); // Binary protocol
                }
                Err(_) => {
                    // Binary parsing failed, fall through to text parsing
                    debug!("Binary protocol parsing failed, trying text protocol");
                }
            }
        }

        // Text protocol parsing (fallback or primary for text commands)
        match ProtocolParser::parse_command(data) {
            Ok(command) => {
                debug!("Using text protocol for command");
                Ok((command, 0)) // Text protocol
            }
            Err(e) => Err(e),
        }
    }
}
