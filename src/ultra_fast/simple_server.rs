//! Simple server implementation for debugging

use crate::Config;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Simple server for debugging
pub struct SimpleServer {
    config: Arc<Config>,
    store: Arc<RwLock<HashMap<String, String>>>,
}

impl SimpleServer {
    /// Create new simple server
    pub async fn new(config: Config) -> crate::Result<Self> {
        info!("SimpleServer initialized for debugging");
        info!("  - Basic HashMap storage");
        info!("  - Standard RwLock synchronization");
        info!("  - Simple protocol parsing");

        Ok(Self {
            config: Arc::new(config),
            store: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Start simple server
    pub async fn start(&self) -> crate::Result<()> {
        let addr = format!("{}:{}", self.config.bind_addr, self.config.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("SimpleServer listening on {}", addr);
        info!("🔧 Debug mode - basic functionality only");

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
                }
            }
        }
    }

    /// Handle single connection
    async fn handle_connection(
        mut stream: TcpStream,
        store: Arc<RwLock<HashMap<String, String>>>,
        client_addr: std::net::SocketAddr,
    ) -> crate::Result<()> {
        debug!("Connection established with {}", client_addr);

        let mut buffer = vec![0u8; 4096];
        let mut command_buffer = Vec::new();

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

            // Process complete commands
            while let Some(newline_pos) = command_buffer.iter().position(|&b| b == b'\n') {
                let command_data = command_buffer.drain(..=newline_pos).collect::<Vec<_>>();
                let command_str = String::from_utf8_lossy(&command_data[..command_data.len() - 1]);
                let command_str = command_str.trim_end_matches('\r');

                if !command_str.is_empty() {
                    if let Err(e) = Self::process_command(command_str, &store, &mut stream).await {
                        error!("Command processing error: {}", e);
                        let _ = stream.write_all(b"ERROR\r\n").await;
                    }
                }
            }
        }

        debug!("Connection with {} closed", client_addr);
        Ok(())
    }

    /// Process single command
    async fn process_command(
        command: &str,
        store: &Arc<RwLock<HashMap<String, String>>>,
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
                let store_guard = store.read().await;

                match store_guard.get(key) {
                    Some(value) => {
                        let response = format!("{}\r\n", value);
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

                let mut store_guard = store.write().await;
                store_guard.insert(key, value);

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
                let mut store_guard = store.write().await;

                match store_guard.remove(key) {
                    Some(_) => stream.write_all(b"OK\r\n").await?,
                    None => stream.write_all(b"NULL\r\n").await?,
                }
            }

            _ => {
                stream.write_all(b"ERROR Unknown command\r\n").await?;
            }
        }

        Ok(())
    }
}
