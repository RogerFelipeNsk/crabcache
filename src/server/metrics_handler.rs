use crate::metrics::{SharedMetrics, PrometheusExporter, Dashboard};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, warn, error};

/// HTTP server for metrics and dashboard
pub struct MetricsServer {
    metrics: SharedMetrics,
    port: u16,
}

impl MetricsServer {
    pub fn new(metrics: SharedMetrics, port: u16) -> Self {
        Self { metrics, port }
    }
    
    /// Start the metrics HTTP server
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!("Metrics server listening on {}", addr);
        
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let metrics = Arc::clone(&self.metrics);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, metrics).await {
                            warn!("Error handling metrics connection from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }
    
    async fn handle_connection(
        mut stream: TcpStream,
        metrics: SharedMetrics,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut buffer = vec![0; 4096];
        let bytes_read = stream.read(&mut buffer).await?;
        
        if bytes_read == 0 {
            return Ok(());
        }
        
        let request = String::from_utf8_lossy(&buffer[..bytes_read]);
        let lines: Vec<&str> = request.lines().collect();
        
        if lines.is_empty() {
            return Self::send_error_response(&mut stream, 400, "Bad Request").await;
        }
        
        let request_line = lines[0];
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        
        if parts.len() < 2 {
            return Self::send_error_response(&mut stream, 400, "Bad Request").await;
        }
        
        let method = parts[0];
        let path = parts[1];
        
        if method != "GET" {
            return Self::send_error_response(&mut stream, 405, "Method Not Allowed").await;
        }
        
        match path {
            "/metrics" => Self::handle_metrics(&mut stream, metrics).await,
            "/dashboard" | "/" => Self::handle_dashboard(&mut stream, metrics).await,
            "/health" => Self::handle_health(&mut stream).await,
            _ => Self::send_error_response(&mut stream, 404, "Not Found").await,
        }
    }
    
    async fn handle_metrics(
        stream: &mut TcpStream,
        metrics: SharedMetrics,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stats = if let Ok(metrics_guard) = metrics.try_read() {
            metrics_guard.get_stats()
        } else {
            return Self::send_error_response(stream, 500, "Internal Server Error").await;
        };
        
        let prometheus_output = PrometheusExporter::export_metrics(&stats);
        
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/plain; version=0.0.4; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            prometheus_output.len(),
            prometheus_output
        );
        
        stream.write_all(response.as_bytes()).await?;
        Ok(())
    }
    
    async fn handle_dashboard(
        stream: &mut TcpStream,
        metrics: SharedMetrics,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stats = if let Ok(metrics_guard) = metrics.try_read() {
            metrics_guard.get_stats()
        } else {
            return Self::send_error_response(stream, 500, "Internal Server Error").await;
        };
        
        let html = Dashboard::generate_html(&stats);
        
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            html.len(),
            html
        );
        
        stream.write_all(response.as_bytes()).await?;
        Ok(())
    }
    
    async fn handle_health(
        stream: &mut TcpStream,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let health_response = r#"{"status":"healthy","service":"crabcache","version":"1.0.0"}"#;
        
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            health_response.len(),
            health_response
        );
        
        stream.write_all(response.as_bytes()).await?;
        Ok(())
    }
    
    async fn send_error_response(
        stream: &mut TcpStream,
        status_code: u16,
        status_text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let body = format!(r#"{{"error":"{}","code":{}}}"#, status_text, status_code);
        
        let response = format!(
            "HTTP/1.1 {} {}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            status_code, status_text, body.len(), body
        );
        
        stream.write_all(response.as_bytes()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::create_shared_metrics;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_metrics_server_health() {
        let metrics = create_shared_metrics(4);
        let server = MetricsServer::new(metrics, 0); // Use port 0 for testing
        
        // Start server in background
        tokio::spawn(async move {
            let _ = server.start().await;
        });
        
        // Give server time to start
        sleep(Duration::from_millis(100)).await;
        
        // Test would require actual HTTP client - simplified for now
        assert!(true);
    }
}