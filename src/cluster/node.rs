//! Cluster node management and representation
//!
//! This module defines the core node structures and capabilities
//! for distributed CrabCache clustering.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;
use uuid::Uuid;

/// Unique identifier for cluster nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(Uuid);

impl NodeId {
    /// Generate a new random node ID
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create node ID from string
    pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the underlying UUID
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Shard identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShardId(pub u32);

impl ShardId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for ShardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "shard_{}", self.0)
    }
}

/// Node status in the cluster
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is joining the cluster
    Joining,
    /// Node is active and serving requests
    Active,
    /// Node is leaving the cluster gracefully
    Leaving,
    /// Node has failed and is not responding
    Failed,
    /// Node is recovering from a failure
    Recovering,
    /// Node is temporarily suspended
    Suspended,
}

impl NodeStatus {
    /// Check if node can serve read requests
    pub fn can_serve_reads(&self) -> bool {
        matches!(self, NodeStatus::Active | NodeStatus::Leaving)
    }

    /// Check if node can serve write requests
    pub fn can_serve_writes(&self) -> bool {
        matches!(self, NodeStatus::Active)
    }

    /// Check if node is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(
            self,
            NodeStatus::Active | NodeStatus::Joining | NodeStatus::Recovering
        )
    }
}

/// Node capabilities and specifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    /// Maximum operations per second this node can handle
    pub max_ops_per_sec: u64,
    /// Total memory capacity in bytes
    pub memory_capacity: u64,
    /// Number of CPU cores
    pub cpu_cores: u32,
    /// SIMD instruction support
    pub simd_support: bool,
    /// Zero-copy operations support
    pub zero_copy_support: bool,
    /// Advanced pipeline support
    pub advanced_pipeline_support: bool,
    /// Supported protocol versions
    pub protocol_versions: Vec<String>,
    /// Node software version
    pub software_version: String,
}

impl Default for NodeCapabilities {
    fn default() -> Self {
        Self {
            max_ops_per_sec: 500_000,
            memory_capacity: 8 * 1024 * 1024 * 1024, // 8GB
            cpu_cores: num_cpus::get() as u32,
            simd_support: true,
            zero_copy_support: true,
            advanced_pipeline_support: true,
            protocol_versions: vec!["1.0".to_string()],
            software_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Performance statistics for a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStats {
    /// Current operations per second
    pub current_ops_per_sec: f64,
    /// Current CPU utilization (0.0 to 1.0)
    pub cpu_utilization: f64,
    /// Current memory utilization (0.0 to 1.0)
    pub memory_utilization: f64,
    /// Current network utilization (0.0 to 1.0)
    pub network_utilization: f64,
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
    /// P99 latency in milliseconds
    pub p99_latency_ms: f64,
    /// Number of active connections
    pub active_connections: u32,
    /// Total requests processed
    pub total_requests: u64,
    /// Total errors encountered
    pub total_errors: u64,
    /// Last update timestamp (as milliseconds since epoch)
    #[serde(with = "instant_serde")]
    pub last_update: Instant,
}

mod instant_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert Instant to milliseconds since epoch (approximation)
        let now = SystemTime::now();
        let epoch_duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
        let millis = epoch_duration.as_millis() as u64;
        millis.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Instant, D::Error>
    where
        D: Deserializer<'de>,
    {
        let _millis = u64::deserialize(deserializer)?;
        // Return current instant as we can't reconstruct the original
        Ok(Instant::now())
    }
}

impl Default for NodeStats {
    fn default() -> Self {
        Self {
            current_ops_per_sec: 0.0,
            cpu_utilization: 0.0,
            memory_utilization: 0.0,
            network_utilization: 0.0,
            avg_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            active_connections: 0,
            total_requests: 0,
            total_errors: 0,
            last_update: Instant::now(),
        }
    }
}

impl NodeStats {
    /// Calculate current load factor (0.0 to 1.0)
    pub fn load_factor(&self) -> f64 {
        // Weighted average of CPU, memory, and throughput utilization
        let cpu_weight = 0.4;
        let memory_weight = 0.3;
        let throughput_weight = 0.3;

        let throughput_utilization = self.current_ops_per_sec / 500_000.0; // Assume 500k max

        (self.cpu_utilization * cpu_weight
            + self.memory_utilization * memory_weight
            + throughput_utilization * throughput_weight)
            .min(1.0)
    }

    /// Check if node is overloaded
    pub fn is_overloaded(&self, threshold: f64) -> bool {
        self.load_factor() > threshold
    }

    /// Get error rate
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_errors as f64 / self.total_requests as f64
        }
    }
}

/// Represents a node in the CrabCache cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterNode {
    /// Unique node identifier
    pub id: NodeId,
    /// Network address for client connections
    pub address: SocketAddr,
    /// Network address for inter-node communication
    pub cluster_address: SocketAddr,
    /// Current node status
    pub status: NodeStatus,
    /// Last heartbeat received (as milliseconds since epoch)
    #[serde(with = "instant_serde")]
    pub last_heartbeat: Instant,
    /// Node capabilities and specifications
    pub capabilities: NodeCapabilities,
    /// Current performance statistics
    pub stats: NodeStats,
    /// Shards assigned to this node
    pub shards: Vec<ShardId>,
    /// Node metadata
    pub metadata: HashMap<String, String>,
}

impl ClusterNode {
    /// Create a new cluster node
    pub fn new(
        id: NodeId,
        address: SocketAddr,
        cluster_address: SocketAddr,
        capabilities: NodeCapabilities,
    ) -> Self {
        Self {
            id,
            address,
            cluster_address,
            status: NodeStatus::Joining,
            last_heartbeat: Instant::now(),
            capabilities,
            stats: NodeStats::default(),
            shards: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Update node statistics
    pub fn update_stats(&mut self, stats: NodeStats) {
        self.stats = stats;
        self.last_heartbeat = Instant::now();
    }

    /// Check if node is considered alive based on heartbeat
    pub fn is_alive(&self, timeout: std::time::Duration) -> bool {
        self.last_heartbeat.elapsed() < timeout
    }

    /// Mark node as failed
    pub fn mark_failed(&mut self) {
        self.status = NodeStatus::Failed;
    }

    /// Mark node as active
    pub fn mark_active(&mut self) {
        self.status = NodeStatus::Active;
        self.last_heartbeat = Instant::now();
    }

    /// Add shard to this node
    pub fn add_shard(&mut self, shard_id: ShardId) {
        if !self.shards.contains(&shard_id) {
            self.shards.push(shard_id);
        }
    }

    /// Remove shard from this node
    pub fn remove_shard(&mut self, shard_id: ShardId) {
        self.shards.retain(|&s| s != shard_id);
    }

    /// Check if node has a specific shard
    pub fn has_shard(&self, shard_id: ShardId) -> bool {
        self.shards.contains(&shard_id)
    }

    /// Get node's current load factor
    pub fn load_factor(&self) -> f64 {
        self.stats.load_factor()
    }

    /// Check if node can accept more load
    pub fn can_accept_load(&self, threshold: f64) -> bool {
        self.status.can_serve_writes() && !self.stats.is_overloaded(threshold)
    }

    /// Get node's effective capacity based on current load
    pub fn effective_capacity(&self) -> u64 {
        let load_factor = self.load_factor();
        let remaining_capacity = 1.0 - load_factor;
        (self.capabilities.max_ops_per_sec as f64 * remaining_capacity) as u64
    }

    /// Set metadata value
    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
}

impl PartialEq for ClusterNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ClusterNode {}

impl std::hash::Hash for ClusterNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_node_id_generation() {
        let id1 = NodeId::generate();
        let id2 = NodeId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_node_id_string_conversion() {
        let id = NodeId::generate();
        let id_str = id.to_string();
        let parsed_id = NodeId::from_string(&id_str).unwrap();
        assert_eq!(id, parsed_id);
    }

    #[test]
    fn test_shard_id() {
        let shard = ShardId::new(42);
        assert_eq!(shard.as_u32(), 42);
        assert_eq!(shard.to_string(), "shard_42");
    }

    #[test]
    fn test_node_status_capabilities() {
        assert!(NodeStatus::Active.can_serve_reads());
        assert!(NodeStatus::Active.can_serve_writes());
        assert!(NodeStatus::Leaving.can_serve_reads());
        assert!(!NodeStatus::Leaving.can_serve_writes());
        assert!(!NodeStatus::Failed.can_serve_reads());
        assert!(!NodeStatus::Failed.can_serve_writes());
    }

    #[test]
    fn test_node_capabilities_default() {
        let caps = NodeCapabilities::default();
        assert!(caps.max_ops_per_sec > 0);
        assert!(caps.memory_capacity > 0);
        assert!(caps.cpu_cores > 0);
        assert!(caps.simd_support);
        assert!(caps.zero_copy_support);
    }

    #[test]
    fn test_node_stats_load_factor() {
        let mut stats = NodeStats::default();
        stats.cpu_utilization = 0.5;
        stats.memory_utilization = 0.3;
        stats.current_ops_per_sec = 250_000.0; // 50% of 500k max

        let load_factor = stats.load_factor();
        assert!(load_factor > 0.0 && load_factor <= 1.0);

        // Should be around 0.45 (weighted average)
        assert!((load_factor - 0.45).abs() < 0.1);
    }

    #[test]
    fn test_node_stats_overload_detection() {
        let mut stats = NodeStats::default();
        stats.cpu_utilization = 0.9;
        stats.memory_utilization = 0.8;
        stats.current_ops_per_sec = 450_000.0;

        assert!(stats.is_overloaded(0.8));
        assert!(!stats.is_overloaded(0.95));
    }

    #[test]
    fn test_cluster_node_creation() {
        let id = NodeId::generate();
        let addr = "127.0.0.1:8000".parse().unwrap();
        let cluster_addr = "127.0.0.1:8001".parse().unwrap();
        let caps = NodeCapabilities::default();

        let node = ClusterNode::new(id, addr, cluster_addr, caps);

        assert_eq!(node.id, id);
        assert_eq!(node.address, addr);
        assert_eq!(node.cluster_address, cluster_addr);
        assert_eq!(node.status, NodeStatus::Joining);
        assert!(node.shards.is_empty());
    }

    #[test]
    fn test_cluster_node_shard_management() {
        let id = NodeId::generate();
        let addr = "127.0.0.1:8000".parse().unwrap();
        let cluster_addr = "127.0.0.1:8001".parse().unwrap();
        let caps = NodeCapabilities::default();

        let mut node = ClusterNode::new(id, addr, cluster_addr, caps);

        let shard1 = ShardId::new(1);
        let shard2 = ShardId::new(2);

        // Add shards
        node.add_shard(shard1);
        node.add_shard(shard2);
        assert_eq!(node.shards.len(), 2);
        assert!(node.has_shard(shard1));
        assert!(node.has_shard(shard2));

        // Remove shard
        node.remove_shard(shard1);
        assert_eq!(node.shards.len(), 1);
        assert!(!node.has_shard(shard1));
        assert!(node.has_shard(shard2));

        // Adding duplicate shard should not increase count
        node.add_shard(shard2);
        assert_eq!(node.shards.len(), 1);
    }

    #[test]
    fn test_cluster_node_heartbeat() {
        let id = NodeId::generate();
        let addr = "127.0.0.1:8000".parse().unwrap();
        let cluster_addr = "127.0.0.1:8001".parse().unwrap();
        let caps = NodeCapabilities::default();

        let node = ClusterNode::new(id, addr, cluster_addr, caps);

        // Node should be alive immediately after creation
        assert!(node.is_alive(Duration::from_secs(10)));

        // Simulate old heartbeat
        let mut old_node = node.clone();
        old_node.last_heartbeat = Instant::now() - Duration::from_secs(20);

        assert!(!old_node.is_alive(Duration::from_secs(10)));
    }

    #[test]
    fn test_cluster_node_load_capacity() {
        let id = NodeId::generate();
        let addr = "127.0.0.1:8000".parse().unwrap();
        let cluster_addr = "127.0.0.1:8001".parse().unwrap();
        let caps = NodeCapabilities::default();

        let mut node = ClusterNode::new(id, addr, cluster_addr, caps);
        node.mark_active();

        // Low load - should accept more
        node.stats.cpu_utilization = 0.3;
        node.stats.memory_utilization = 0.2;
        node.stats.current_ops_per_sec = 100_000.0;

        assert!(node.can_accept_load(0.8));
        assert!(node.effective_capacity() > 300_000);

        // High load - should not accept more
        node.stats.cpu_utilization = 0.9;
        node.stats.memory_utilization = 0.8;
        node.stats.current_ops_per_sec = 450_000.0;

        assert!(!node.can_accept_load(0.8));
        assert!(node.effective_capacity() < 100_000);
    }

    #[test]
    fn test_cluster_node_metadata() {
        let id = NodeId::generate();
        let addr = "127.0.0.1:8000".parse().unwrap();
        let cluster_addr = "127.0.0.1:8001".parse().unwrap();
        let caps = NodeCapabilities::default();

        let mut node = ClusterNode::new(id, addr, cluster_addr, caps);

        // Set and get metadata
        node.set_metadata("region".to_string(), "us-west-1".to_string());
        node.set_metadata("zone".to_string(), "us-west-1a".to_string());

        assert_eq!(node.get_metadata("region"), Some(&"us-west-1".to_string()));
        assert_eq!(node.get_metadata("zone"), Some(&"us-west-1a".to_string()));
        assert_eq!(node.get_metadata("nonexistent"), None);
    }

    #[test]
    fn test_node_equality_and_hashing() {
        let id1 = NodeId::generate();
        let id2 = NodeId::generate();
        let addr = "127.0.0.1:8000".parse().unwrap();
        let cluster_addr = "127.0.0.1:8001".parse().unwrap();
        let caps = NodeCapabilities::default();

        let node1a = ClusterNode::new(id1, addr, cluster_addr, caps.clone());
        let node1b = ClusterNode::new(id1, addr, cluster_addr, caps.clone());
        let node2 = ClusterNode::new(id2, addr, cluster_addr, caps);

        // Nodes with same ID should be equal
        assert_eq!(node1a, node1b);
        assert_ne!(node1a, node2);

        // Hash should be based on ID
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher1a = DefaultHasher::new();
        let mut hasher1b = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();

        node1a.hash(&mut hasher1a);
        node1b.hash(&mut hasher1b);
        node2.hash(&mut hasher2);

        assert_eq!(hasher1a.finish(), hasher1b.finish());
        assert_ne!(hasher1a.finish(), hasher2.finish());
    }
}
