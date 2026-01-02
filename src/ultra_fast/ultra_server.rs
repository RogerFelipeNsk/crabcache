//! Ultra-fast server implementation targeting 500k ops/sec with P99 < 10ms

use crate::ultra_fast::{
    arena_allocator::arena_reset,
    assembly_optimized::{cpu_pause, prefetch_data},
    lockfree_shard_manager::LockFreeShardManager,
    response_pool::{
        get_error_response, get_null_response, get_ok_response, get_pong_response,
        get_value_response,
    },
    simd_parser::parse_command_simd,
    zero_copy_parser::CommandRef,
};
use crate::Config;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

/// Ultra-fast server with extreme optimizations
pub struct UltraFastServer {
    config: Arc<Config>,
    shard_manager: Arc<LockFreeShardManager>,
    stats: UltraServerStats,
}

/// Ultra-fast server statistics
#[derive(Debug, Default)]
pub struct UltraServerStats {
    pub total_connections: std::sync::atomic::AtomicU64,
    pub total_commands: std::sync::atomic::AtomicU64,
    pub total_errors: std::sync::atomic::AtomicU64,
    pub arena_resets: std::sync::atomic::AtomicU64,
    pub zero_copy_hits: std::sync::atomic::AtomicU64,
    pub response_pool_hits: std::sync::atomic::AtomicU64,
}

impl UltraFastServer {
    /// Create new ultra-fast server
    pub async fn new(config: Config) -> crate::Result<Self> {
        let num_shards = config.get_num_shards();
        let max_memory_per_shard = config.max_memory_per_shard;

        let shard_manager = Arc::new(LockFreeShardManager::new(num_shards, max_memory_per_shard));

        info!("UltraFastServer initialized:");
        info!("  - Target: 500k+ ops/sec, P99 < 10ms");
        info!("  - Lock-free shard manager enabled");
        info!("  - SIMD parsing enabled (Sprint 2)");
        info!("  - Assembly optimizations enabled");
        info!("  - Zero-copy parsing enabled");
        info!("  - Arena allocator enabled");
        info!("  - Response pool enabled");
        info!(
            "  - {} shards, {}B per shard",
            num_shards, max_memory_per_shard
        );

        Ok(Self {
            config: Arc::new(config),
            shard_manager,
            stats: UltraServerStats::default(),
        })
    }

    /// Start ultra-fast server
    pub async fn start(&self) -> crate::Result<()> {
        let addr = format!("{}:{}", self.config.bind_addr, self.config.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("UltraFastServer listening on {}", addr);
        info!("🚀 Ready for ultra-high performance!");

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("New ultra-fast connection from {}", addr);

                    let shard_manager = Arc::clone(&self.shard_manager);
                    let stats = &self.stats;

                    // Increment connection counter
                    stats
                        .total_connections
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::handle_ultra_fast_connection(stream, shard_manager, addr).await
                        {
                            error!("Ultra-fast connection error from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Handle single connection with ultra-fast optimizations
    async fn handle_ultra_fast_connection(
        mut stream: TcpStream,
        shard_manager: Arc<LockFreeShardManager>,
        client_addr: std::net::SocketAddr,
    ) -> crate::Result<()> {
        // Ultra-fast socket optimizations
        if let Err(e) = stream.set_nodelay(true) {
            warn!("Failed to set TCP_NODELAY: {}", e);
        }

        // Pre-allocate buffer for ultra-fast I/O with prefetching
        let mut buffer = vec![0u8; 65536]; // 64KB buffer
        let mut command_buffer = Vec::with_capacity(4096);

        // Prefetch buffer into CPU cache
        unsafe {
            prefetch_data(buffer.as_ptr(), 0);
        }

        debug!("Ultra-fast connection established with {}", client_addr);

        loop {
            // Reset arena allocator periodically to prevent memory growth
            arena_reset();

            // Read data with timeout
            let n = match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                stream.read(&mut buffer),
            )
            .await
            {
                Ok(Ok(0)) => {
                    debug!("Client {} disconnected", client_addr);
                    break;
                }
                Ok(Ok(n)) => n,
                Ok(Err(e)) => {
                    error!("Read error from {}: {}", client_addr, e);
                    break;
                }
                Err(_) => {
                    warn!("Read timeout from {}", client_addr);
                    // Use CPU pause for better power efficiency
                    cpu_pause();
                    break;
                }
            };

            // Append to command buffer
            command_buffer.extend_from_slice(&buffer[..n]);

            // Process all complete commands with zero-copy parsing
            while let Some(newline_pos) = command_buffer.iter().position(|&b| b == b'\n') {
                let command_data = command_buffer.drain(..=newline_pos).collect::<Vec<_>>();
                let command_bytes = &command_data[..command_data.len() - 1]; // Remove newline

                // Remove \r if present
                let command_bytes = if command_bytes.ends_with(b"\r") {
                    &command_bytes[..command_bytes.len() - 1]
                } else {
                    command_bytes
                };

                if !command_bytes.is_empty() {
                    if let Err(e) = Self::process_ultra_fast_command(
                        command_bytes,
                        &shard_manager,
                        &mut stream,
                        client_addr,
                    )
                    .await
                    {
                        error!("Command processing error: {}", e);
                        // Send error response but don't break connection
                        let error_response = get_error_response("PROCESSING_ERROR");
                        let _ = stream.write_all(&error_response).await;
                    }
                }
            }
        }

        debug!("Ultra-fast connection with {} closed", client_addr);
        Ok(())
    }

    /// Process single command with ultra-fast optimizations
    #[inline(always)]
    async fn process_ultra_fast_command(
        data: &[u8],
        shard_manager: &Arc<LockFreeShardManager>,
        stream: &mut TcpStream,
        client_addr: std::net::SocketAddr,
    ) -> crate::Result<()> {
        let start_time = Instant::now();

        // SIMD-optimized zero-copy parsing (ultra-fast)
        let command_ref = match parse_command_simd(data) {
            Ok(cmd) => cmd,
            Err(e) => {
                debug!("Parse error from {}: {}", client_addr, e);
                let error_response = get_error_response("PARSE_ERROR");
                stream.write_all(&error_response).await?;
                return Ok(());
            }
        };

        // Process command with ultra-fast response generation
        let response_bytes = match command_ref {
            CommandRef::Ping => {
                // Ultra-fast static response (zero allocation)
                get_pong_response()
            }

            CommandRef::Get { key: _ } => {
                // Use zero-copy processing directly
                let response = shard_manager.process_command_zero_copy(command_ref).await;

                match response {
                    crate::protocol::Response::Value(value) => {
                        // Use response pool for common value sizes
                        let response_bytes = get_value_response(&value);
                        return stream.write_all(&response_bytes).await.map_err(Into::into);
                    }
                    crate::protocol::Response::Null => get_null_response(),
                    _ => {
                        // Fallback to error
                        let error_bytes = get_error_response("UNEXPECTED_RESPONSE");
                        return stream.write_all(&error_bytes).await.map_err(Into::into);
                    }
                }
            }

            CommandRef::Put { .. } | CommandRef::Del { .. } | CommandRef::Expire { .. } => {
                // Use zero-copy processing directly
                let response = shard_manager.process_command_zero_copy(command_ref).await;

                match response {
                    crate::protocol::Response::Ok => get_ok_response(),
                    crate::protocol::Response::Null => get_null_response(),
                    _ => {
                        let error_bytes = get_error_response("UNEXPECTED_RESPONSE");
                        return stream.write_all(&error_bytes).await.map_err(Into::into);
                    }
                }
            }

            CommandRef::Stats | CommandRef::Metrics => {
                // Use zero-copy processing for stats
                let response = shard_manager.process_command_zero_copy(command_ref).await;

                match response {
                    crate::protocol::Response::Stats(stats) => {
                        // Create response for stats (not pooled due to dynamic content)
                        let mut response_bytes = Vec::with_capacity(stats.len() + 2);
                        response_bytes.extend_from_slice(stats.as_bytes());
                        response_bytes.extend_from_slice(b"\r\n");
                        return stream.write_all(&response_bytes).await.map_err(Into::into);
                    }
                    _ => {
                        let error_bytes = get_error_response("STATS_ERROR");
                        return stream.write_all(&error_bytes).await.map_err(Into::into);
                    }
                }
            }
        };

        // Write response (ultra-fast)
        stream.write_all(response_bytes).await?;

        let latency = start_time.elapsed();
        if latency.as_millis() > 1 {
            debug!(
                "Slow command from {}: {}ms",
                client_addr,
                latency.as_millis()
            );
        }

        Ok(())
    }

    /// Get server statistics
    pub fn get_stats(&self) -> UltraServerStats {
        UltraServerStats {
            total_connections: std::sync::atomic::AtomicU64::new(
                self.stats
                    .total_connections
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            total_commands: std::sync::atomic::AtomicU64::new(
                self.stats
                    .total_commands
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            total_errors: std::sync::atomic::AtomicU64::new(
                self.stats
                    .total_errors
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            arena_resets: std::sync::atomic::AtomicU64::new(
                self.stats
                    .arena_resets
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            zero_copy_hits: std::sync::atomic::AtomicU64::new(
                self.stats
                    .zero_copy_hits
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            response_pool_hits: std::sync::atomic::AtomicU64::new(
                self.stats
                    .response_pool_hits
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ultra_fast_server_creation() {
        let config = Config::default();
        let server = UltraFastServer::new(config).await;
        assert!(server.is_ok());
    }

    #[tokio::test]
    async fn test_zero_copy_parsing() {
        use crate::ultra_fast::zero_copy_parser::{CommandRef, ZeroCopyParser};
        let data = b"PING";
        let result = ZeroCopyParser::parse_zero_copy(data);
        assert!(matches!(result, Ok(CommandRef::Ping)));
    }

    #[tokio::test]
    async fn test_response_pool() {
        let ok_response = get_ok_response();
        assert_eq!(ok_response, b"OK\r\n");

        let pong_response = get_pong_response();
        assert_eq!(pong_response, b"PONG\r\n");

        let value_response = get_value_response(b"test");
        assert!(value_response.starts_with(b"VALUE "));
    }
}
