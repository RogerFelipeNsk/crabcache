//! Cluster management and distributed coordination
//!
//! This module implements the distributed clustering functionality for CrabCache Phase 7:
//! - Node discovery and cluster membership
//! - Consistent hashing and auto-sharding
//! - Raft consensus protocol
//! - Distributed pipeline processing

use serde::{Deserialize, Serialize};

pub mod consensus;
pub mod discovery;
pub mod distributed_pipeline;
pub mod hash_ring;
pub mod load_balancer;
pub mod migration;
pub mod node;

pub use consensus::{RaftLog, RaftNode, RaftState};
pub use discovery::{ClusterManager, ServiceDiscovery};
pub use distributed_pipeline::{CrossNodeRouter, DistributedPipelineManager};
pub use hash_ring::{ConsistentHashRing, MigrationStatus, ShardId, ShardMigration};
pub use load_balancer::{LoadBalancer, LoadBalancingStrategy};
pub use migration::MigrationExecutor;
pub use node::{ClusterNode, NodeCapabilities, NodeId, NodeStatus};

/// Cluster configuration
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    /// Local node configuration
    pub node_id: NodeId,
    pub bind_address: std::net::SocketAddr,
    pub advertise_address: std::net::SocketAddr,

    /// Cluster settings
    pub cluster_name: String,
    pub seed_nodes: Vec<std::net::SocketAddr>,
    pub replication_factor: u32,
    pub virtual_nodes: u32,

    /// Consensus settings
    pub election_timeout_ms: u64,
    pub heartbeat_interval_ms: u64,
    pub max_log_entries: u64,

    /// Performance settings
    pub max_concurrent_migrations: u32,
    pub migration_batch_size: u32,
    pub load_balance_threshold: f64,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            node_id: NodeId::generate(),
            bind_address: "0.0.0.0:8000".parse().unwrap(),
            advertise_address: "127.0.0.1:8000".parse().unwrap(),
            cluster_name: "crabcache-cluster".to_string(),
            seed_nodes: vec![],
            replication_factor: 3,
            virtual_nodes: 256,
            election_timeout_ms: 5000,
            heartbeat_interval_ms: 1000,
            max_log_entries: 10000,
            max_concurrent_migrations: 3,
            migration_batch_size: 1000,
            load_balance_threshold: 0.2,
        }
    }
}

/// Cluster-wide metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClusterMetrics {
    /// Node count and health
    pub total_nodes: u32,
    pub active_nodes: u32,
    pub failed_nodes: u32,

    /// Performance metrics
    pub cluster_throughput: f64,
    pub avg_node_load: f64,
    pub max_node_load: f64,
    pub min_node_load: f64,

    /// Consensus metrics
    pub leader_node: Option<NodeId>,
    pub leader_election_count: u64,
    pub consensus_latency_ms: f64,

    /// Migration metrics
    pub active_migrations: u32,
    pub completed_migrations: u64,
    pub failed_migrations: u64,

    /// Network metrics
    pub cross_node_latency_ms: f64,
    pub network_utilization: f64,
    pub message_rate: f64,
}

/// Result type for cluster operations
pub type ClusterResult<T> = Result<T, ClusterError>;

/// Cluster operation errors
#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    #[error("Node not found: {node_id}")]
    NodeNotFound { node_id: NodeId },

    #[error("Cluster not ready: {reason}")]
    ClusterNotReady { reason: String },

    #[error("Consensus error: {message}")]
    ConsensusError { message: String },

    #[error("Migration failed: {migration_id}")]
    MigrationFailed { migration_id: String },

    #[error("Network error: {source}")]
    NetworkError { source: std::io::Error },

    #[error("Configuration error: {message}")]
    ConfigError { message: String },

    #[error("Timeout: {operation}")]
    Timeout { operation: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_config_default() {
        let config = ClusterConfig::default();
        assert_eq!(config.replication_factor, 3);
        assert_eq!(config.virtual_nodes, 256);
        assert!(!config.cluster_name.is_empty());
    }

    #[test]
    fn test_cluster_metrics_default() {
        let metrics = ClusterMetrics::default();
        assert_eq!(metrics.total_nodes, 0);
        assert_eq!(metrics.active_nodes, 0);
        assert_eq!(metrics.cluster_throughput, 0.0);
    }
}
