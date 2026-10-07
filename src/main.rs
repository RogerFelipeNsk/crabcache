use clap::Parser;
use crabcache::config::Config;
use crabcache::server::{Server, log_startup};
use std::process::ExitCode;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// `mi_option_purge_delay` in the `mi_option_e` enum of the bundled mimalloc v3 (`c_src/mimalloc/v3/include/mimalloc.h`);
/// libmimalloc-sys does not export it by name.
const MI_OPTION_PURGE_DELAY: libmimalloc_sys::mi_option_t = 15;

/// mimalloc returns freed pages to the OS only when the owning thread allocates again after the purge
/// delay, so memory released while a shard's vector or hash table grew stayed resident on idle I/O
/// threads (~14 bytes per key with 1M small keys). Purging immediately keeps the footprint at what is
/// actually in use.
fn configure_allocator() -> i64 {
    // SAFETY: plain option getters/setters with no preconditions.
    unsafe {
        let default = libmimalloc_sys::mi_option_get(MI_OPTION_PURGE_DELAY) as i64;
        libmimalloc_sys::mi_option_set(MI_OPTION_PURGE_DELAY, 0);
        default
    }
}

fn main() -> ExitCode {
    let purge_default = configure_allocator();
    let config = Config::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    tracing::debug!(purge_default, "mimalloc purge delay set to 0 ms");

    // The main runtime only accepts connections and runs maintenance; I/O threads are started by
    // `Server::run`.
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            error!(error = %e, "failed to start runtime");
            return ExitCode::FAILURE;
        }
    };

    runtime.block_on(async move {
        let server = match Server::bind(config.clone()).await {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, bind = %config.bind, port = config.port, "failed to bind");
                return ExitCode::FAILURE;
            }
        };
        match server.local_addr() {
            Ok(addr) => log_startup(&config, addr),
            Err(e) => error!(error = %e, "could not read local address"),
        }
        server.run(shutdown_signal()).await;
        info!("shutting down");
        ExitCode::SUCCESS
    })
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = term => {}
    }
}
