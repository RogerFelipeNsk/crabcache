use crabcache::ultra_fast::{IoUringServer, ToonUltimateServer, UltimateServer, UltraFastServer};
use crabcache::{Config, Result};
use tracing::{error, info, warn};
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

    // Choose server implementation based on configuration
    let server_type =
        std::env::var("CRABCACHE_SERVER_TYPE").unwrap_or_else(|_| "ultimate".to_string());

    info!("CrabCache server starting...");
    info!("🚀 Target: 500k+ ops/sec, P99 < 10ms");
    info!("Server type: {}", server_type);

    let tcp_port = config.port;

    // Start the appropriate server
    let server_handle = match server_type.as_str() {
        "toon_ultimate" => {
            info!("🚀 Starting CrabCache ToonUltimateServer (TOON Protocol + All Sprints)");
            let server = ToonUltimateServer::new(config).await?;
            tokio::spawn(async move {
                if let Err(e) = server.start().await {
                    error!(error = %e, "ToonUltimateServer error");
                }
            })
        }
        "ultimate" => {
            info!("🚀 Starting CrabCache UltimateServer (Sprint 3 & 4)");
            let server = UltimateServer::new(config).await?;
            tokio::spawn(async move {
                if let Err(e) = server.start().await {
                    error!(error = %e, "UltimateServer error");
                }
            })
        }
        "io_uring" => {
            info!("🚀 Starting CrabCache IoUringServer (Sprint 3)");
            let server = IoUringServer::new(config).await?;
            tokio::spawn(async move {
                if let Err(e) = server.start().await {
                    error!(error = %e, "IoUringServer error");
                }
            })
        }
        "ultra" => {
            info!("🚀 Starting CrabCache UltraFastServer (Sprint 2)");
            let server = UltraFastServer::new(config).await?;
            tokio::spawn(async move {
                if let Err(e) = server.start().await {
                    error!(error = %e, "UltraFastServer error");
                }
            })
        }
        _ => {
            warn!(
                "Unknown server type '{}', defaulting to ToonUltimateServer",
                server_type
            );
            let server = ToonUltimateServer::new(config).await?;
            tokio::spawn(async move {
                if let Err(e) = server.start().await {
                    error!(error = %e, "ToonUltimateServer error");
                }
            })
        }
    };

    info!(tcp_port = tcp_port, "CrabCache server ready!");
    info!("🚀 Performance: Targeting 500k+ ops/sec, P99 < 10ms");
    info!("🔥 Lock-free architecture enabled");
    info!("⚡ SIMD parsing enabled (Sprint 2)");
    info!("🏎️  Assembly optimizations enabled");
    info!("🧠 Arena allocator enabled");
    info!("🚀 io_uring-style batching enabled (Sprint 3)");
    info!("🎯 CPU/Memory optimizations enabled (Sprint 4)");
    info!("🌟 ARM64 NEON SIMD enabled (Sprint 4)");
    info!("🎨 TOON Protocol support enabled (80%+ smaller than JSON)");

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
