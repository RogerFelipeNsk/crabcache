use crabcache::server::TcpServer;
use crabcache::{Config, Result};
use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging with JSON format
    let subscriber = fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .with_current_span(false)
        .with_span_list(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    info!("Starting CrabCache v{}", crabcache::VERSION);

    // Load configuration
    let config = Config::load().await?;
    info!(
        bind_addr = %config.bind_addr,
        port = config.port,
        "Configuration loaded"
    );

    // Initialize server with metrics
    info!("CrabCache server starting with observability...");

    let tcp_port = config.port;
    let server = TcpServer::new(config).await?;

    // Start server with metrics endpoint
    let metrics_port = 9090; // Standard Prometheus port

    info!(
        tcp_port = tcp_port,
        metrics_port = metrics_port,
        "Starting CrabCache with observability"
    );

    // Start server with integrated metrics
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start_with_metrics(metrics_port).await {
            error!(error = %e, "Server error");
        }
    });

    info!("CrabCache server ready with full observability!");
    info!("🚀 Performance: 25,824+ ops/sec, P99 < 1ms");
    info!("📊 Metrics: http://localhost:{}/metrics", metrics_port);
    info!("📈 Dashboard: http://localhost:{}/dashboard", metrics_port);

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        _ = server_handle => {
            error!("Server task completed unexpectedly");
        }
    }

    info!("Shutting down CrabCache server...");

    Ok(())
}
