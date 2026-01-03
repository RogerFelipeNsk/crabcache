//! TOON Hybrid server - extends HybridServer with basic TOON protocol support

use crate::metrics::{create_shared_metrics, SharedMetrics};
use crate::server::MetricsServer;
use crate::Config;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info};

/// TOON Hybrid server - stable with basic TOON protocol support
pub struct ToonHybridServer {
    config: Arc<Config>,
    store: Arc<DashMap<String, String>>,
    metrics: SharedMetrics,
}

impl ToonHybridServer {
    /// Create new TOON hybrid server
    pub async fn new(config: Config) -> crate::Result<Self> {
        info!("ToonHybridServer initialized");
        info!("  - DashMap storage (lock-free)");
        info!("  - Basic TOON protocol support");
        info!("  - Stable TCP handling");
        info!("  - Text protocol fallback");
        info!("  - Metrics server enabled");

        let metrics = create_shared_metrics(1); // Single shard for simplicity

        Ok(Self {
            config: Arc::new(config),
            store: Arc::new(DashMap::new()),
            metrics,
        })
    }

    /// Start TOON hybrid server
    pub async fn start(&self) -> crate::Result<()> {
        let addr = format!("{}:{}", self.config.bind_addr, self.config.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("ToonHybridServer listening on {}", addr);
        info!("🎨 TOON protocol ready (with text fallback)");

        // Start metrics server
        let metrics_server = MetricsServer::new(Arc::clone(&self.metrics), 9090);
        let metrics_handle = tokio::spawn(async move {
            if let Err(e) = metrics_server.start().await {
                error!("Metrics server error: {}", e);
            }
        });

        info!("Started metrics server on port 9090");
        info!("Available endpoints:");
        info!("  - http://localhost:9090/metrics (Prometheus)");
        info!("  - http://localhost:9090/dashboard (Web UI)");
        info!("  - http://localhost:9090/health (Health check)");

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("New connection from {}", addr);

                    let store = Arc::clone(&self.store);

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, store, addr).await {
                            error!("Connection error from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    // If main server stops, stop metrics server too
                    metrics_handle.abort();
                    return Err(e.into());
                }
            }
        }
    }

    /// Handle single connection with TOON protocol support
    async fn handle_connection(
        mut stream: TcpStream,
        store: Arc<DashMap<String, String>>,
        client_addr: std::net::SocketAddr,
    ) -> crate::Result<()> {
        debug!("TOON connection established with {}", client_addr);

        // Optimized socket settings
        if let Err(e) = stream.set_nodelay(true) {
            debug!("Failed to set TCP_NODELAY: {}", e);
        }

        // Larger buffer for better performance
        let mut buffer = vec![0u8; 8192]; // 8KB buffer
        let mut command_buffer = Vec::with_capacity(1024);

        loop {
            let n = match stream.read(&mut buffer).await {
                Ok(0) => {
                    debug!("Client {} disconnected", client_addr);
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    error!("Read error from {}: {}", client_addr, e);
                    break;
                }
            };

            command_buffer.extend_from_slice(&buffer[..n]);

            // Check for TOON protocol first
            if command_buffer.len() >= 4 && &command_buffer[0..4] == b"TOON" {
                // Try TOON protocol processing
                if let Err(e) =
                    Self::process_toon_protocol(&command_buffer, &store, &mut stream).await
                {
                    debug!("TOON processing failed, falling back to text: {}", e);
                    // Clear buffer and continue with text protocol
                    command_buffer.clear();
                }
            } else {
                // Process text commands (same as HybridServer)
                while let Some(newline_pos) = command_buffer.iter().position(|&b| b == b'\n') {
                    let command_data = command_buffer.drain(..=newline_pos).collect::<Vec<_>>();
                    let command_str =
                        String::from_utf8_lossy(&command_data[..command_data.len() - 1]);
                    let command_str = command_str.trim_end_matches('\r');

                    if !command_str.is_empty() {
                        if let Err(e) =
                            Self::process_text_command(command_str, &store, &mut stream).await
                        {
                            error!("Command processing error: {}", e);
                            let _ = stream.write_all(b"ERROR\r\n").await;
                        }
                    }
                }
            }
        }

        debug!("TOON connection with {} closed", client_addr);
        Ok(())
    }

    /// Process TOON protocol (simplified implementation)
    async fn process_toon_protocol(
        _buffer: &[u8],
        store: &Arc<DashMap<String, String>>,
        stream: &mut TcpStream,
    ) -> crate::Result<()> {
        // For now, just respond with a simple TOON-style response
        // This is a placeholder for future TOON protocol implementation

        // Send a simple TOON response indicating protocol is recognized
        let toon_response = b"TOON\x01\x00\x01\x00"; // Magic + version + flags + minimal data
        stream.write_all(toon_response).await?;

        // Also send stats in text format for now
        let count = store.len();
        let response = format!("TOON_STATS keys: {}\r\n", count);
        stream.write_all(response.as_bytes()).await?;

        Ok(())
    }

    /// Process text command (same as HybridServer)
    async fn process_text_command(
        command: &str,
        store: &Arc<DashMap<String, String>>,
        stream: &mut TcpStream,
    ) -> crate::Result<()> {
        let parts: Vec<&str> = command.split_whitespace().collect();

        if parts.is_empty() {
            stream.write_all(b"ERROR Empty command\r\n").await?;
            return Ok(());
        }

        match parts[0].to_uppercase().as_str() {
            "PING" => {
                stream.write_all(b"PONG\r\n").await?;
            }

            "GET" => {
                if parts.len() != 2 {
                    stream
                        .write_all(b"ERROR Wrong number of arguments\r\n")
                        .await?;
                    return Ok(());
                }

                let key = parts[1];

                // DashMap is lock-free and very fast
                match store.get(key) {
                    Some(value) => {
                        let response = format!("{}\r\n", value.value());
                        stream.write_all(response.as_bytes()).await?;
                    }
                    None => {
                        stream.write_all(b"NULL\r\n").await?;
                    }
                }
            }

            "PUT" => {
                if parts.len() != 3 {
                    stream
                        .write_all(b"ERROR Wrong number of arguments\r\n")
                        .await?;
                    return Ok(());
                }

                let key = parts[1].to_string();
                let value = parts[2].to_string();

                // DashMap insert is lock-free
                store.insert(key, value);

                stream.write_all(b"OK\r\n").await?;
            }

            "DEL" => {
                if parts.len() != 2 {
                    stream
                        .write_all(b"ERROR Wrong number of arguments\r\n")
                        .await?;
                    return Ok(());
                }

                let key = parts[1];

                // DashMap remove is lock-free
                match store.remove(key) {
                    Some(_) => stream.write_all(b"OK\r\n").await?,
                    None => stream.write_all(b"NULL\r\n").await?,
                }
            }

            "STATS" => {
                let count = store.len();
                let response = format!("keys: {}\r\n", count);
                stream.write_all(response.as_bytes()).await?;
            }

            "TOON_TEST" => {
                // Special command to test TOON protocol recognition
                stream.write_all(b"TOON_READY\r\n").await?;
            }

            _ => {
                stream.write_all(b"ERROR Unknown command\r\n").await?;
            }
        }

        Ok(())
    }
}
