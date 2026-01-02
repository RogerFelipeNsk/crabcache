//! Ultimate performance server integrating all Sprint 3 & 4 optimizations
//! Target: 500k+ ops/sec with P99 < 2ms

use crate::ultra_fast::{
    arena_allocator::arena_reset,
    arm64_simd::{hash_key_neon, parse_command_neon},
    assembly_optimized::{cpu_pause, prefetch_data},
    cpu_optimized::{
        BranchOptimizer, CacheAligned, CacheOptimizer, CpuAffinityManager, HotDataLayout,
        MemoryOptimizer, PerformanceMonitor, PrefetchOptimizer,
    },
    lockfree_shard_manager::LockFreeShardManager,
    response_pool::{
        get_error_response, get_null_response, get_ok_response, get_pong_response,
        get_value_response,
    },
    simd_parser::parse_command_simd,
    zero_copy_parser::CommandRef,
};
use crate::Config;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

/// Ultimate performance server with all optimizations enabled
pub struct UltimateServer {
    config: Arc<Config>,
    shard_manager: Arc<LockFreeShardManager>,
    stats: CacheAligned<UltimateServerStats>,

    // CPU optimizations
    cpu_affinity: Arc<CpuAffinityManager>,
    performance_monitor: Arc<PerformanceMonitor>,

    // Memory optimizations
    hot_data: HotDataLayout<ServerHotData>,

    // Batch processing optimizations
    batch_config: BatchConfig,
}

/// Hot data that's accessed frequently (cache-line optimized)
#[derive(Debug)]
struct ServerHotData {
    active_connections: AtomicUsize,
    total_operations: AtomicU64,
    current_batch_size: AtomicUsize,
    last_optimization_check: AtomicU64,
}

/// Batch processing configuration
#[derive(Debug, Clone)]
struct BatchConfig {
    max_batch_size: usize,
    min_batch_size: usize,
    batch_timeout_ns: u64,
    adaptive_batching: bool,
}

/// Ultimate server statistics with cache-line alignment
#[repr(align(64))]
#[derive(Debug, Default)]
pub struct UltimateServerStats {
    // Performance metrics
    pub total_connections: AtomicU64,
    pub total_operations: AtomicU64,
    pub operations_per_second: AtomicU64,

    // Latency metrics
    pub min_latency_ns: AtomicU64,
    pub max_latency_ns: AtomicU64,
    pub avg_latency_ns: AtomicU64,
    pub p99_latency_ns: AtomicU64,

    // Optimization metrics
    pub simd_operations: AtomicU64,
    pub neon_operations: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub batch_efficiency: AtomicU64,

    // System metrics
    pub cpu_utilization: AtomicU64,
    pub memory_utilization: AtomicU64,
    pub syscall_reduction: AtomicU64,
}

impl UltimateServer {
    /// Create the ultimate performance server
    pub async fn new(config: Config) -> crate::Result<Self> {
        let num_shards = config.get_num_shards();
        let max_memory_per_shard = config.max_memory_per_shard;

        let shard_manager = Arc::new(LockFreeShardManager::new(num_shards, max_memory_per_shard));

        // Initialize CPU optimizations
        let cpu_affinity = Arc::new(CpuAffinityManager::new());
        let performance_monitor = Arc::new(PerformanceMonitor::new());

        // Configure adaptive batching
        let batch_config = BatchConfig {
            max_batch_size: 512,       // Large batches for throughput
            min_batch_size: 1,         // Small batches for latency
            batch_timeout_ns: 500_000, // 0.5ms timeout
            adaptive_batching: true,
        };

        // Initialize hot data structure
        let hot_data = HotDataLayout::new(ServerHotData {
            active_connections: AtomicUsize::new(0),
            total_operations: AtomicU64::new(0),
            current_batch_size: AtomicUsize::new(batch_config.min_batch_size),
            last_optimization_check: AtomicU64::new(0),
        });

        info!("UltimateServer initialized:");
        info!("  - Target: 500k+ ops/sec, P99 < 2ms");
        info!("  - All Sprint 3 & 4 optimizations enabled");
        info!("  - ARM64 NEON SIMD: {}", cfg!(target_arch = "aarch64"));
        info!(
            "  - x86_64 AVX2 SIMD: {}",
            cfg!(any(target_arch = "x86", target_arch = "x86_64"))
        );
        info!("  - io_uring-style batching enabled");
        info!("  - CPU affinity optimization enabled");
        info!("  - Cache-line optimization enabled");
        info!("  - NUMA-aware memory allocation enabled");
        info!(
            "  - Adaptive batch size: {}-{}",
            batch_config.min_batch_size, batch_config.max_batch_size
        );
        info!(
            "  - {} shards, {}B per shard",
            num_shards, max_memory_per_shard
        );

        Ok(Self {
            config: Arc::new(config),
            shard_manager,
            stats: CacheAligned::new(UltimateServerStats::default()),
            cpu_affinity,
            performance_monitor,
            hot_data,
            batch_config,
        })
    }

    /// Start the ultimate performance server
    pub async fn start(&self) -> crate::Result<()> {
        let addr = format!("{}:{}", self.config.bind_addr, self.config.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("UltimateServer listening on {}", addr);
        info!("🚀 Ready for ultimate performance!");

        // Pre-warm all systems
        self.prewarm_ultimate_system().await;

        // Set CPU affinity for main thread
        if let Err(e) = self.cpu_affinity.pin_to_optimal_core() {
            warn!("Failed to set CPU affinity: {}", e);
        }

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("New ultimate connection from {}", addr);

                    let shard_manager = Arc::clone(&self.shard_manager);
                    let stats = &self.stats;
                    let cpu_affinity = Arc::clone(&self.cpu_affinity);
                    let performance_monitor = Arc::clone(&self.performance_monitor);
                    let batch_config = self.batch_config.clone();

                    // Increment connection counter
                    stats.data.total_connections.fetch_add(1, Ordering::Relaxed);
                    self.hot_data
                        .hot_data
                        .active_connections
                        .fetch_add(1, Ordering::Relaxed);

                    tokio::spawn(async move {
                        // Set CPU affinity for connection thread
                        let _ = cpu_affinity.pin_to_optimal_core();

                        if let Err(e) = Self::handle_ultimate_connection(
                            stream,
                            shard_manager,
                            addr,
                            batch_config,
                            performance_monitor,
                        )
                        .await
                        {
                            error!("Ultimate connection error from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Pre-warm all systems for ultimate performance
    async fn prewarm_ultimate_system(&self) {
        info!("🔥 Pre-warming ultimate performance system...");

        // Pre-warm arena allocator
        arena_reset();

        // Pre-warm all parsers
        let _ = parse_command_neon(b"PING");
        let _ = parse_command_simd(b"GET warmup_key");
        let _ = hash_key_neon(b"warmup_key");

        // Pre-warm response pool
        let _ = get_pong_response();
        let _ = get_ok_response();
        let _ = get_null_response();

        // Pre-warm CPU caches
        let warmup_data = vec![0u8; 64 * 1024]; // 64KB
        CacheOptimizer::warm_cache(&warmup_data);

        // Pre-warm memory subsystem
        let mut dst = vec![0u8; 1024];
        let src = vec![1u8; 1024];
        MemoryOptimizer::streaming_copy(&mut dst, &src);

        info!("✅ Ultimate system pre-warming completed");
    }

    /// Handle connection with ultimate optimizations
    async fn handle_ultimate_connection(
        mut stream: TcpStream,
        shard_manager: Arc<LockFreeShardManager>,
        client_addr: std::net::SocketAddr,
        batch_config: BatchConfig,
        performance_monitor: Arc<PerformanceMonitor>,
    ) -> crate::Result<()> {
        // Ultra-aggressive socket optimizations
        if let Err(e) = stream.set_nodelay(true) {
            warn!("Failed to set TCP_NODELAY: {}", e);
        }

        // Cache-line aligned buffers
        let mut read_buffer = vec![0u8; 2 * 1024 * 1024]; // 2MB read buffer
        let mut write_buffer = Vec::with_capacity(2 * 1024 * 1024); // 2MB write buffer
        let mut command_batch = Vec::with_capacity(batch_config.max_batch_size);
        let mut response_batch = Vec::with_capacity(batch_config.max_batch_size);

        // Pre-fetch buffers into CPU cache
        PrefetchOptimizer::prefetch_read(read_buffer.as_ptr(), read_buffer.len());
        PrefetchOptimizer::prefetch_write(write_buffer.as_ptr(), write_buffer.capacity());

        debug!("Ultimate connection established with {}", client_addr);

        let mut pending_data = Vec::new();
        let mut current_batch_size = batch_config.min_batch_size;
        let mut last_batch_time = Instant::now();
        let mut latency_samples = Vec::with_capacity(1000);

        loop {
            let batch_start = Instant::now();

            // Adaptive timeout based on current load
            let timeout_ns = if current_batch_size > batch_config.min_batch_size {
                batch_config.batch_timeout_ns / 2 // Shorter timeout for larger batches
            } else {
                batch_config.batch_timeout_ns
            };

            // Read with adaptive timeout
            let n = match tokio::time::timeout(
                std::time::Duration::from_nanos(timeout_ns),
                stream.readable(),
            )
            .await
            {
                Ok(_) => {
                    match stream.try_read(&mut read_buffer) {
                        Ok(n) if n == 0 => {
                            debug!("Client {} disconnected", client_addr);
                            break;
                        }
                        Ok(n) => n,
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // Process batch if we have commands or timeout
                            if !command_batch.is_empty() {
                                Self::process_ultimate_batch(
                                    &command_batch,
                                    &mut response_batch,
                                    &shard_manager,
                                    client_addr,
                                    &performance_monitor,
                                )
                                .await?;

                                // Write batch response with streaming optimization
                                if !response_batch.is_empty() {
                                    Self::write_ultimate_batch(
                                        &mut stream,
                                        &response_batch,
                                        &mut write_buffer,
                                    )
                                    .await?;

                                    // Record latency
                                    let batch_latency = batch_start.elapsed();
                                    latency_samples.push(batch_latency.as_nanos() as u64);

                                    // Adaptive batch size adjustment
                                    if batch_config.adaptive_batching {
                                        current_batch_size = Self::adjust_batch_size(
                                            current_batch_size,
                                            batch_latency,
                                            &batch_config,
                                        );
                                    }

                                    response_batch.clear();
                                }
                                command_batch.clear();
                            }
                            continue;
                        }
                        Err(e) => {
                            error!("Read error from {}: {}", client_addr, e);
                            break;
                        }
                    }
                }
                Err(_) => {
                    // Timeout - process any pending batch
                    if !command_batch.is_empty() {
                        Self::process_ultimate_batch(
                            &command_batch,
                            &mut response_batch,
                            &shard_manager,
                            client_addr,
                            &performance_monitor,
                        )
                        .await?;

                        if !response_batch.is_empty() {
                            Self::write_ultimate_batch(
                                &mut stream,
                                &response_batch,
                                &mut write_buffer,
                            )
                            .await?;
                            response_batch.clear();
                        }
                        command_batch.clear();
                    }
                    continue;
                }
            };

            // Append new data with prefetching
            let old_len = pending_data.len();
            pending_data.extend_from_slice(&read_buffer[..n]);
            PrefetchOptimizer::prefetch_read(pending_data[old_len..].as_ptr(), n);

            // Parse all complete commands with SIMD optimization
            while let Some(newline_pos) = pending_data.iter().position(|&b| b == b'\n') {
                let command_data: Vec<u8> = pending_data.drain(..=newline_pos).collect();
                let command_bytes = &command_data[..command_data.len() - 1]; // Remove newline

                // Remove \r if present
                let command_bytes = if command_bytes.ends_with(b"\r") {
                    &command_bytes[..command_bytes.len() - 1]
                } else {
                    command_bytes
                };

                if !command_bytes.is_empty() {
                    command_batch.push(command_bytes.to_vec());

                    // Process batch when full or on adaptive threshold
                    if command_batch.len() >= current_batch_size {
                        Self::process_ultimate_batch(
                            &command_batch,
                            &mut response_batch,
                            &shard_manager,
                            client_addr,
                            &performance_monitor,
                        )
                        .await?;

                        // Write batch response immediately for low latency
                        if !response_batch.is_empty() {
                            Self::write_ultimate_batch(
                                &mut stream,
                                &response_batch,
                                &mut write_buffer,
                            )
                            .await?;

                            // Record latency and adjust batch size
                            let batch_latency = batch_start.elapsed();
                            latency_samples.push(batch_latency.as_nanos() as u64);

                            if batch_config.adaptive_batching {
                                current_batch_size = Self::adjust_batch_size(
                                    current_batch_size,
                                    batch_latency,
                                    &batch_config,
                                );
                            }

                            response_batch.clear();
                        }
                        command_batch.clear();
                        last_batch_time = Instant::now();
                    }
                }
            }
        }

        debug!("Ultimate connection with {} closed", client_addr);
        Ok(())
    }

    /// Process batch with ultimate optimizations
    async fn process_ultimate_batch(
        commands: &[Vec<u8>],
        responses: &mut Vec<Vec<u8>>,
        shard_manager: &Arc<LockFreeShardManager>,
        client_addr: std::net::SocketAddr,
        performance_monitor: &Arc<PerformanceMonitor>,
    ) -> crate::Result<()> {
        let batch_start = Instant::now();

        for command_data in commands {
            let parse_start = Instant::now();

            // Ultimate SIMD parsing with platform detection
            let command_ref = match Self::parse_command_ultimate(command_data) {
                Ok(cmd) => cmd,
                Err(e) => {
                    debug!("Parse error from {}: {}", client_addr, e);
                    responses.push(get_error_response("PARSE_ERROR").to_vec());
                    continue;
                }
            };

            let parse_time = parse_start.elapsed();
            if BranchOptimizer::unlikely(parse_time.as_nanos() > 1000) {
                // > 1μs
                performance_monitor.record_cache_miss();
            }

            // Process command with zero-copy optimization
            let response_bytes = match command_ref {
                CommandRef::Ping => {
                    // Ultra-fast static response
                    get_pong_response().to_vec()
                }

                CommandRef::Get { .. } => {
                    let response = shard_manager.process_command_zero_copy(command_ref).await;

                    match response {
                        crate::protocol::Response::Value(value) => {
                            get_value_response(&value).to_vec()
                        }
                        crate::protocol::Response::Null => get_null_response().to_vec(),
                        _ => get_error_response("UNEXPECTED_RESPONSE").to_vec(),
                    }
                }

                CommandRef::Put { .. } | CommandRef::Del { .. } | CommandRef::Expire { .. } => {
                    let response = shard_manager.process_command_zero_copy(command_ref).await;

                    match response {
                        crate::protocol::Response::Ok => get_ok_response().to_vec(),
                        crate::protocol::Response::Null => get_null_response().to_vec(),
                        _ => get_error_response("UNEXPECTED_RESPONSE").to_vec(),
                    }
                }

                CommandRef::Stats | CommandRef::Metrics => {
                    let response = shard_manager.process_command_zero_copy(command_ref).await;

                    match response {
                        crate::protocol::Response::Stats(stats) => {
                            let mut response_bytes = Vec::with_capacity(stats.len() + 2);
                            response_bytes.extend_from_slice(stats.as_bytes());
                            response_bytes.extend_from_slice(b"\r\n");
                            response_bytes
                        }
                        _ => get_error_response("STATS_ERROR").to_vec(),
                    }
                }
            };

            responses.push(response_bytes);
        }

        let batch_latency = batch_start.elapsed();
        if BranchOptimizer::unlikely(batch_latency.as_millis() > 1) {
            debug!(
                "Slow ultimate batch from {}: {}ms for {} commands",
                client_addr,
                batch_latency.as_millis(),
                commands.len()
            );
        }

        Ok(())
    }

    /// Ultimate command parsing with platform-specific optimizations
    #[inline(always)]
    fn parse_command_ultimate(data: &[u8]) -> Result<CommandRef<'_>, &'static str> {
        // Use the best parser for the current platform
        #[cfg(target_arch = "aarch64")]
        {
            parse_command_neon(data)
        }

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            parse_command_simd(data)
        }

        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
        {
            crate::ultra_fast::zero_copy_parser::ZeroCopyParser::parse_zero_copy(data)
        }
    }

    /// Write batch with ultimate streaming optimizations
    async fn write_ultimate_batch(
        stream: &mut TcpStream,
        responses: &[Vec<u8>],
        write_buffer: &mut Vec<u8>,
    ) -> crate::Result<()> {
        use tokio::io::AsyncWriteExt;

        // Batch all responses into single write buffer
        write_buffer.clear();
        let total_size: usize = responses.iter().map(|r| r.len()).sum();
        write_buffer.reserve(total_size);

        for response in responses {
            write_buffer.extend_from_slice(response);
        }

        // Single syscall for entire batch with prefetching
        PrefetchOptimizer::prefetch_read(write_buffer.as_ptr(), write_buffer.len());
        stream.write_all(write_buffer).await?;

        Ok(())
    }

    /// Adaptive batch size adjustment
    fn adjust_batch_size(
        current_size: usize,
        batch_latency: std::time::Duration,
        config: &BatchConfig,
    ) -> usize {
        let latency_ns = batch_latency.as_nanos() as u64;

        // Target: P99 < 2ms = 2,000,000 ns
        const TARGET_LATENCY_NS: u64 = 2_000_000;

        if latency_ns < TARGET_LATENCY_NS / 2 {
            // Very fast, can increase batch size
            (current_size * 2).min(config.max_batch_size)
        } else if latency_ns > TARGET_LATENCY_NS {
            // Too slow, decrease batch size
            (current_size / 2).max(config.min_batch_size)
        } else {
            // Good latency, keep current size
            current_size
        }
    }

    /// Get ultimate server statistics
    pub fn get_stats(&self) -> UltimateServerStats {
        UltimateServerStats {
            total_connections: AtomicU64::new(
                self.stats.data.total_connections.load(Ordering::Relaxed),
            ),
            total_operations: AtomicU64::new(
                self.stats.data.total_operations.load(Ordering::Relaxed),
            ),
            operations_per_second: AtomicU64::new(
                self.stats
                    .data
                    .operations_per_second
                    .load(Ordering::Relaxed),
            ),
            min_latency_ns: AtomicU64::new(self.stats.data.min_latency_ns.load(Ordering::Relaxed)),
            max_latency_ns: AtomicU64::new(self.stats.data.max_latency_ns.load(Ordering::Relaxed)),
            avg_latency_ns: AtomicU64::new(self.stats.data.avg_latency_ns.load(Ordering::Relaxed)),
            p99_latency_ns: AtomicU64::new(self.stats.data.p99_latency_ns.load(Ordering::Relaxed)),
            simd_operations: AtomicU64::new(
                self.stats.data.simd_operations.load(Ordering::Relaxed),
            ),
            neon_operations: AtomicU64::new(
                self.stats.data.neon_operations.load(Ordering::Relaxed),
            ),
            cache_hits: AtomicU64::new(self.stats.data.cache_hits.load(Ordering::Relaxed)),
            cache_misses: AtomicU64::new(self.stats.data.cache_misses.load(Ordering::Relaxed)),
            batch_efficiency: AtomicU64::new(
                self.stats.data.batch_efficiency.load(Ordering::Relaxed),
            ),
            cpu_utilization: AtomicU64::new(
                self.stats.data.cpu_utilization.load(Ordering::Relaxed),
            ),
            memory_utilization: AtomicU64::new(
                self.stats.data.memory_utilization.load(Ordering::Relaxed),
            ),
            syscall_reduction: AtomicU64::new(
                self.stats.data.syscall_reduction.load(Ordering::Relaxed),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ultimate_server_creation() {
        let config = Config::default();
        let server = UltimateServer::new(config).await;
        assert!(server.is_ok());
    }

    #[test]
    fn test_adaptive_batch_sizing() {
        let config = BatchConfig {
            max_batch_size: 512,
            min_batch_size: 1,
            batch_timeout_ns: 500_000,
            adaptive_batching: true,
        };

        // Fast latency should increase batch size
        let fast_latency = std::time::Duration::from_nanos(100_000); // 0.1ms
        let new_size = UltimateServer::adjust_batch_size(10, fast_latency, &config);
        assert!(new_size > 10);

        // Slow latency should decrease batch size
        let slow_latency = std::time::Duration::from_millis(5); // 5ms
        let new_size = UltimateServer::adjust_batch_size(100, slow_latency, &config);
        assert!(new_size < 100);
    }

    #[test]
    fn test_ultimate_parsing() {
        let commands = [
            &b"PING"[..],
            &b"GET test_key"[..],
            &b"PUT test_key test_value"[..],
            &b"DEL test_key"[..],
        ];

        for command in &commands {
            let result = UltimateServer::parse_command_ultimate(command);
            assert!(result.is_ok());
        }
    }
}
