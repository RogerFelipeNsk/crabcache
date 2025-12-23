pub mod collector;
pub mod prometheus;
pub mod dashboard;
pub mod histogram;

pub use collector::{MetricsCollector, ShardMetrics, GlobalMetrics};
pub use prometheus::PrometheusExporter;
pub use dashboard::Dashboard;
pub use histogram::LatencyHistogram;

use std::sync::Arc;
use tokio::sync::RwLock;

/// Global metrics instance
pub type SharedMetrics = Arc<RwLock<MetricsCollector>>;

/// Create a new shared metrics collector
pub fn create_shared_metrics(num_shards: usize) -> SharedMetrics {
    Arc::new(RwLock::new(MetricsCollector::new(num_shards)))
}