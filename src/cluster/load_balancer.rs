//! Load balancer for distributing requests across cluster nodes
//!
//! This module implements intelligent load balancing strategies to optimize
//! performance and resource utilization across the CrabCache cluster.

use crate::cluster::{ClusterError, ClusterNode, ClusterResult, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Load balancing strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadBalancingStrategy {
    /// Round-robin distribution
    RoundRobin,
    /// Weighted round-robin based on node capacity
    WeightedRoundRobin,
    /// Least connections
    LeastConnections,
    /// Least response time
    LeastResponseTime,
    /// Resource-based (CPU, memory, throughput)
    ResourceBased,
    /// Consistent hashing with load awareness
    ConsistentHashWithLoad,
    /// Adaptive strategy that switches based on conditions
    Adaptive {
        primary: Box<LoadBalancingStrategy>,
        fallback: Box<LoadBalancingStrategy>,
        switch_threshold: f64,
    },
}

/// Node health status for load balancing
#[derive(Debug, Clone, PartialEq)]
pub enum NodeHealth {
    Healthy,
    Degraded,
    Unhealthy,
    Draining,
}

impl Default for NodeHealth {
    fn default() -> Self {
        NodeHealth::Healthy
    }
}

/// Load balancing metrics for a node
#[derive(Debug, Clone)]
pub struct NodeLoadMetrics {
    /// Current active connections
    pub active_connections: u32,
    /// Average response time in milliseconds
    pub avg_response_time: f64,
    /// Current CPU utilization (0.0 to 1.0)
    pub cpu_utilization: f64,
    /// Current memory utilization (0.0 to 1.0)
    pub memory_utilization: f64,
    /// Current throughput (ops/sec)
    pub throughput: f64,
    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,
    /// Health status
    pub health: NodeHealth,
    /// Last update timestamp
    pub last_update: Instant,
}

impl Default for NodeLoadMetrics {
    fn default() -> Self {
        Self {
            active_connections: 0,
            avg_response_time: 0.0,
            cpu_utilization: 0.0,
            memory_utilization: 0.0,
            throughput: 0.0,
            error_rate: 0.0,
            health: NodeHealth::Healthy,
            last_update: Instant::now(),
        }
    }
}

impl NodeLoadMetrics {
    /// Calculate overall load score (0.0 = no load, 1.0 = maximum load)
    pub fn load_score(&self) -> f64 {
        let cpu_weight = 0.3;
        let memory_weight = 0.2;
        let connection_weight = 0.2;
        let response_time_weight = 0.2;
        let error_weight = 0.1;

        let connection_score = (self.active_connections as f64 / 1000.0).min(1.0);
        let response_time_score = (self.avg_response_time / 100.0).min(1.0); // 100ms = 1.0

        (self.cpu_utilization * cpu_weight
            + self.memory_utilization * memory_weight
            + connection_score * connection_weight
            + response_time_score * response_time_weight
            + self.error_rate * error_weight)
            .min(1.0)
    }

    /// Check if node can accept new requests
    pub fn can_accept_requests(&self) -> bool {
        matches!(self.health, NodeHealth::Healthy | NodeHealth::Degraded) && self.load_score() < 0.9
        // Don't send to nodes over 90% load
    }

    /// Check if node is overloaded
    pub fn is_overloaded(&self) -> bool {
        self.load_score() > 0.8 || self.health == NodeHealth::Unhealthy
    }
}

/// Load balancer implementation
pub struct LoadBalancer {
    /// Load balancing strategy
    strategy: LoadBalancingStrategy,
    /// Node metrics tracking
    node_metrics: Arc<RwLock<HashMap<NodeId, NodeLoadMetrics>>>,
    /// Round-robin counter
    round_robin_counter: Arc<RwLock<usize>>,
    /// Strategy-specific state
    strategy_state: Arc<RwLock<StrategyState>>,
    /// Load balancer metrics
    metrics: Arc<RwLock<LoadBalancerMetrics>>,
}

/// Strategy-specific state
#[derive(Debug, Default)]
struct StrategyState {
    /// Weighted round-robin weights
    weights: HashMap<NodeId, u32>,
    /// Weighted round-robin current weights
    current_weights: HashMap<NodeId, i32>,
    /// Adaptive strategy current mode
    adaptive_mode: Option<LoadBalancingStrategy>,
}

/// Load balancer metrics
#[derive(Debug, Clone, Default)]
pub struct LoadBalancerMetrics {
    /// Total requests balanced
    pub total_requests: u64,
    /// Requests per node
    pub requests_per_node: HashMap<NodeId, u64>,
    /// Load balancing decisions
    pub balancing_decisions: u64,
    /// Average decision time
    pub avg_decision_time: f64,
    /// Load distribution efficiency (0.0 to 1.0)
    pub distribution_efficiency: f64,
    /// Failed routing attempts
    pub failed_routings: u64,
}

impl LoadBalancer {
    /// Create new load balancer
    pub fn new(strategy: LoadBalancingStrategy) -> Self {
        Self {
            strategy,
            node_metrics: Arc::new(RwLock::new(HashMap::new())),
            round_robin_counter: Arc::new(RwLock::new(0)),
            strategy_state: Arc::new(RwLock::new(StrategyState::default())),
            metrics: Arc::new(RwLock::new(LoadBalancerMetrics::default())),
        }
    }

    /// Select best node for a request
    pub async fn select_node(&self, available_nodes: &[ClusterNode]) -> ClusterResult<NodeId> {
        let start_time = Instant::now();

        if available_nodes.is_empty() {
            return Err(ClusterError::ClusterNotReady {
                reason: "No nodes available".to_string(),
            });
        }

        // Filter healthy nodes
        let healthy_nodes = self.filter_healthy_nodes(available_nodes).await;

        if healthy_nodes.is_empty() {
            return Err(ClusterError::ClusterNotReady {
                reason: "No healthy nodes available".to_string(),
            });
        }

        // Select node based on strategy
        let selected_node = match &self.strategy {
            LoadBalancingStrategy::RoundRobin => self.select_round_robin(&healthy_nodes).await,
            LoadBalancingStrategy::WeightedRoundRobin => {
                self.select_weighted_round_robin(&healthy_nodes).await
            }
            LoadBalancingStrategy::LeastConnections => {
                self.select_least_connections(&healthy_nodes).await
            }
            LoadBalancingStrategy::LeastResponseTime => {
                self.select_least_response_time(&healthy_nodes).await
            }
            LoadBalancingStrategy::ResourceBased => {
                self.select_resource_based(&healthy_nodes).await
            }
            LoadBalancingStrategy::ConsistentHashWithLoad => {
                self.select_consistent_hash_with_load(&healthy_nodes).await
            }
            LoadBalancingStrategy::Adaptive {
                primary,
                fallback,
                switch_threshold,
            } => {
                self.select_adaptive(&healthy_nodes, primary, fallback, *switch_threshold)
                    .await
            }
        }?;

        // Update metrics
        let decision_time = start_time.elapsed();
        self.update_metrics(selected_node, decision_time).await;

        Ok(selected_node)
    }

    /// Filter nodes that can accept requests
    async fn filter_healthy_nodes(&self, nodes: &[ClusterNode]) -> Vec<ClusterNode> {
        let metrics = self.node_metrics.read().await;

        nodes
            .iter()
            .filter(|node| {
                if let Some(node_metrics) = metrics.get(&node.id) {
                    node_metrics.can_accept_requests()
                } else {
                    // If no metrics available, assume healthy
                    true
                }
            })
            .cloned()
            .collect()
    }

    /// Round-robin selection
    async fn select_round_robin(&self, nodes: &[ClusterNode]) -> ClusterResult<NodeId> {
        let mut counter = self.round_robin_counter.write().await;
        let index = *counter % nodes.len();
        *counter = (*counter + 1) % nodes.len();

        Ok(nodes[index].id)
    }

    /// Weighted round-robin selection
    async fn select_weighted_round_robin(&self, nodes: &[ClusterNode]) -> ClusterResult<NodeId> {
        let mut state = self.strategy_state.write().await;

        // Initialize weights if not set
        for node in nodes {
            if !state.weights.contains_key(&node.id) {
                let weight = (node.capabilities.max_ops_per_sec / 100_000) as u32; // Scale down
                state.weights.insert(node.id, weight.max(1));
                state.current_weights.insert(node.id, 0);
            }
        }

        // Find node with highest current weight
        let mut best_node = nodes[0].id;
        let mut best_weight = state.current_weights.get(&best_node).copied().unwrap_or(0);

        for node in nodes {
            let current_weight = state.current_weights.get(&node.id).copied().unwrap_or(0);
            if current_weight > best_weight {
                best_node = node.id;
                best_weight = current_weight;
            }
        }

        // Update weights
        let total_weight: i32 = state.weights.values().map(|&w| w as i32).sum();
        for node in nodes {
            let weight = state.weights.get(&node.id).copied().unwrap_or(1) as i32;
            let current = state.current_weights.get_mut(&node.id).unwrap();

            if node.id == best_node {
                *current -= total_weight - weight;
            } else {
                *current += weight;
            }
        }

        Ok(best_node)
    }

    /// Least connections selection
    async fn select_least_connections(&self, nodes: &[ClusterNode]) -> ClusterResult<NodeId> {
        let metrics = self.node_metrics.read().await;

        let mut best_node = nodes[0].id;
        let mut best_connections = metrics
            .get(&best_node)
            .map(|m| m.active_connections)
            .unwrap_or(0);

        for node in &nodes[1..] {
            let connections = metrics
                .get(&node.id)
                .map(|m| m.active_connections)
                .unwrap_or(0);

            if connections < best_connections {
                best_node = node.id;
                best_connections = connections;
            }
        }

        Ok(best_node)
    }

    /// Least response time selection
    async fn select_least_response_time(&self, nodes: &[ClusterNode]) -> ClusterResult<NodeId> {
        let metrics = self.node_metrics.read().await;

        let mut best_node = nodes[0].id;
        let mut best_response_time = metrics
            .get(&best_node)
            .map(|m| m.avg_response_time)
            .unwrap_or(0.0);

        for node in &nodes[1..] {
            let response_time = metrics
                .get(&node.id)
                .map(|m| m.avg_response_time)
                .unwrap_or(0.0);

            if response_time < best_response_time {
                best_node = node.id;
                best_response_time = response_time;
            }
        }

        Ok(best_node)
    }

    /// Resource-based selection
    async fn select_resource_based(&self, nodes: &[ClusterNode]) -> ClusterResult<NodeId> {
        let metrics = self.node_metrics.read().await;

        let mut best_node = nodes[0].id;
        let mut best_score = metrics
            .get(&best_node)
            .map(|m| m.load_score())
            .unwrap_or(0.0);

        for node in &nodes[1..] {
            let score = metrics.get(&node.id).map(|m| m.load_score()).unwrap_or(0.0);

            if score < best_score {
                best_node = node.id;
                best_score = score;
            }
        }

        Ok(best_node)
    }

    /// Consistent hash with load awareness
    async fn select_consistent_hash_with_load(
        &self,
        nodes: &[ClusterNode],
    ) -> ClusterResult<NodeId> {
        // For now, just use resource-based selection
        // In a full implementation, this would use consistent hashing with load factors
        self.select_resource_based(nodes).await
    }

    /// Adaptive selection
    async fn select_adaptive(
        &self,
        nodes: &[ClusterNode],
        primary: &LoadBalancingStrategy,
        fallback: &LoadBalancingStrategy,
        switch_threshold: f64,
    ) -> ClusterResult<NodeId> {
        let metrics = self.node_metrics.read().await;

        // Calculate average load across all nodes
        let total_load: f64 = metrics.values().map(|m| m.load_score()).sum();
        let avg_load = if metrics.is_empty() {
            0.0
        } else {
            total_load / metrics.len() as f64
        };

        drop(metrics);

        // Choose strategy based on load
        let strategy = if avg_load > switch_threshold {
            fallback
        } else {
            primary
        };

        // Use the chosen strategy directly instead of creating a new balancer
        match strategy {
            LoadBalancingStrategy::RoundRobin => self.select_round_robin(nodes).await,
            LoadBalancingStrategy::WeightedRoundRobin => {
                self.select_weighted_round_robin(nodes).await
            }
            LoadBalancingStrategy::LeastConnections => self.select_least_connections(nodes).await,
            LoadBalancingStrategy::LeastResponseTime => {
                self.select_least_response_time(nodes).await
            }
            LoadBalancingStrategy::ResourceBased => self.select_resource_based(nodes).await,
            LoadBalancingStrategy::ConsistentHashWithLoad => {
                self.select_consistent_hash_with_load(nodes).await
            }
            LoadBalancingStrategy::Adaptive { .. } => {
                // Prevent infinite recursion by falling back to resource-based
                self.select_resource_based(nodes).await
            }
        }
    }

    /// Update node metrics
    pub async fn update_node_metrics(&self, node_id: NodeId, metrics: NodeLoadMetrics) {
        let mut node_metrics = self.node_metrics.write().await;
        node_metrics.insert(node_id, metrics);
    }

    /// Update load balancer metrics
    async fn update_metrics(&self, selected_node: NodeId, decision_time: Duration) {
        let mut metrics = self.metrics.write().await;

        metrics.total_requests += 1;
        *metrics.requests_per_node.entry(selected_node).or_insert(0) += 1;
        metrics.balancing_decisions += 1;

        let decision_time_ms = decision_time.as_secs_f64() * 1000.0;
        metrics.avg_decision_time = (metrics.avg_decision_time
            * (metrics.balancing_decisions - 1) as f64
            + decision_time_ms)
            / metrics.balancing_decisions as f64;

        // Calculate distribution efficiency
        if !metrics.requests_per_node.is_empty() {
            let total_requests = metrics.requests_per_node.values().sum::<u64>() as f64;
            let node_count = metrics.requests_per_node.len() as f64;
            let expected_per_node = total_requests / node_count;

            let variance: f64 = metrics
                .requests_per_node
                .values()
                .map(|&count| {
                    let diff = count as f64 - expected_per_node;
                    diff * diff
                })
                .sum::<f64>()
                / node_count;

            let std_dev = variance.sqrt();
            let coefficient_of_variation = if expected_per_node > 0.0 {
                std_dev / expected_per_node
            } else {
                0.0
            };

            // Lower coefficient of variation = better distribution
            metrics.distribution_efficiency = (1.0 - coefficient_of_variation.min(1.0)).max(0.0);
        }
    }

    /// Get load balancer metrics
    pub async fn get_metrics(&self) -> LoadBalancerMetrics {
        self.metrics.read().await.clone()
    }

    /// Get node metrics
    pub async fn get_node_metrics(&self) -> HashMap<NodeId, NodeLoadMetrics> {
        self.node_metrics.read().await.clone()
    }

    /// Mark node as unhealthy
    pub async fn mark_node_unhealthy(&self, node_id: NodeId) {
        let mut metrics = self.node_metrics.write().await;
        if let Some(node_metrics) = metrics.get_mut(&node_id) {
            node_metrics.health = NodeHealth::Unhealthy;
            warn!("Marked node {} as unhealthy", node_id);
        }
    }

    /// Mark node as healthy
    pub async fn mark_node_healthy(&self, node_id: NodeId) {
        let mut metrics = self.node_metrics.write().await;
        if let Some(node_metrics) = metrics.get_mut(&node_id) {
            node_metrics.health = NodeHealth::Healthy;
            info!("Marked node {} as healthy", node_id);
        }
    }

    /// Start draining a node (stop sending new requests)
    pub async fn start_draining_node(&self, node_id: NodeId) {
        let mut metrics = self.node_metrics.write().await;
        if let Some(node_metrics) = metrics.get_mut(&node_id) {
            node_metrics.health = NodeHealth::Draining;
            info!("Started draining node {}", node_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::node::{NodeCapabilities, NodeId};

    fn create_test_node(id: u32, max_ops: u64) -> ClusterNode {
        let node_id = NodeId::generate();
        let addr = format!("127.0.0.1:{}", 8000 + id).parse().unwrap();
        let cluster_addr = format!("127.0.0.1:{}", 9000 + id).parse().unwrap();
        let mut capabilities = NodeCapabilities::default();
        capabilities.max_ops_per_sec = max_ops;

        crate::cluster::ClusterNode::new(node_id, addr, cluster_addr, capabilities)
    }

    #[tokio::test]
    async fn test_load_balancer_creation() {
        let balancer = LoadBalancer::new(LoadBalancingStrategy::RoundRobin);
        let metrics = balancer.get_metrics().await;
        assert_eq!(metrics.total_requests, 0);
    }

    #[tokio::test]
    async fn test_round_robin_selection() {
        let balancer = LoadBalancer::new(LoadBalancingStrategy::RoundRobin);

        let node1 = create_test_node(1, 500_000);
        let node2 = create_test_node(2, 500_000);
        let node3 = create_test_node(3, 500_000);
        let nodes = vec![node1.clone(), node2.clone(), node3.clone()];

        // Test round-robin distribution
        let mut selections = HashMap::new();
        for _ in 0..9 {
            let selected = balancer.select_node(&nodes).await.unwrap();
            *selections.entry(selected).or_insert(0) += 1;
        }

        // Each node should be selected 3 times
        assert_eq!(selections.len(), 3);
        for count in selections.values() {
            assert_eq!(*count, 3);
        }
    }

    #[tokio::test]
    async fn test_least_connections_selection() {
        let balancer = LoadBalancer::new(LoadBalancingStrategy::LeastConnections);

        let node1 = create_test_node(1, 500_000);
        let node2 = create_test_node(2, 500_000);
        let nodes = vec![node1.clone(), node2.clone()];

        // Set different connection counts
        let mut metrics1 = NodeLoadMetrics::default();
        metrics1.active_connections = 10;
        metrics1.health = NodeHealth::Healthy;

        let mut metrics2 = NodeLoadMetrics::default();
        metrics2.active_connections = 5;
        metrics2.health = NodeHealth::Healthy;

        balancer.update_node_metrics(node1.id, metrics1).await;
        balancer.update_node_metrics(node2.id, metrics2).await;

        // Should select node2 (fewer connections)
        let selected = balancer.select_node(&nodes).await.unwrap();
        assert_eq!(selected, node2.id);
    }

    #[tokio::test]
    async fn test_resource_based_selection() {
        let balancer = LoadBalancer::new(LoadBalancingStrategy::ResourceBased);

        let node1 = create_test_node(1, 500_000);
        let node2 = create_test_node(2, 500_000);
        let nodes = vec![node1.clone(), node2.clone()];

        // Set different resource utilization
        let mut metrics1 = NodeLoadMetrics::default();
        metrics1.cpu_utilization = 0.8;
        metrics1.memory_utilization = 0.7;
        metrics1.health = NodeHealth::Healthy;

        let mut metrics2 = NodeLoadMetrics::default();
        metrics2.cpu_utilization = 0.3;
        metrics2.memory_utilization = 0.2;
        metrics2.health = NodeHealth::Healthy;

        balancer.update_node_metrics(node1.id, metrics1).await;
        balancer.update_node_metrics(node2.id, metrics2).await;

        // Should select node2 (lower resource usage)
        let selected = balancer.select_node(&nodes).await.unwrap();
        assert_eq!(selected, node2.id);
    }

    #[tokio::test]
    async fn test_weighted_round_robin() {
        let balancer = LoadBalancer::new(LoadBalancingStrategy::WeightedRoundRobin);

        let node1 = create_test_node(1, 300_000); // Lower capacity
        let node2 = create_test_node(2, 600_000); // Higher capacity
        let nodes = vec![node1.clone(), node2.clone()];

        // Test weighted distribution over many selections
        let mut selections = HashMap::new();
        for _ in 0..30 {
            let selected = balancer.select_node(&nodes).await.unwrap();
            *selections.entry(selected).or_insert(0) += 1;
        }

        // Node2 should be selected more often due to higher capacity
        let node1_count = selections.get(&node1.id).copied().unwrap_or(0);
        let node2_count = selections.get(&node2.id).copied().unwrap_or(0);

        assert!(node2_count > node1_count);
    }

    #[tokio::test]
    async fn test_unhealthy_node_filtering() {
        let balancer = LoadBalancer::new(LoadBalancingStrategy::RoundRobin);

        let node1 = create_test_node(1, 500_000);
        let node2 = create_test_node(2, 500_000);
        let nodes = vec![node1.clone(), node2.clone()];

        // Mark node1 as unhealthy
        let mut metrics1 = NodeLoadMetrics::default();
        metrics1.health = NodeHealth::Unhealthy;

        let mut metrics2 = NodeLoadMetrics::default();
        metrics2.health = NodeHealth::Healthy;

        balancer.update_node_metrics(node1.id, metrics1).await;
        balancer.update_node_metrics(node2.id, metrics2).await;

        // Should only select healthy node
        for _ in 0..5 {
            let selected = balancer.select_node(&nodes).await.unwrap();
            assert_eq!(selected, node2.id);
        }
    }

    #[test]
    fn test_node_load_metrics() {
        let mut metrics = NodeLoadMetrics::default();
        metrics.cpu_utilization = 0.5;
        metrics.memory_utilization = 0.3;
        metrics.active_connections = 100;
        metrics.avg_response_time = 50.0;
        metrics.error_rate = 0.01;

        let load_score = metrics.load_score();
        assert!(load_score > 0.0 && load_score < 1.0);

        assert!(metrics.can_accept_requests());
        assert!(!metrics.is_overloaded());

        // Test overloaded condition
        metrics.cpu_utilization = 0.95;
        assert!(metrics.is_overloaded());
    }

    #[tokio::test]
    async fn test_adaptive_strategy() {
        let primary = Box::new(LoadBalancingStrategy::RoundRobin);
        let fallback = Box::new(LoadBalancingStrategy::LeastConnections);
        let adaptive = LoadBalancingStrategy::Adaptive {
            primary,
            fallback,
            switch_threshold: 0.7,
        };

        let balancer = LoadBalancer::new(adaptive);

        let node1 = create_test_node(1, 500_000);
        let node2 = create_test_node(2, 500_000);
        let nodes = vec![node1.clone(), node2.clone()];

        // Set high load to trigger fallback strategy
        let mut metrics1 = NodeLoadMetrics::default();
        metrics1.cpu_utilization = 0.9;
        metrics1.health = NodeHealth::Healthy;

        let mut metrics2 = NodeLoadMetrics::default();
        metrics2.cpu_utilization = 0.8;
        metrics2.health = NodeHealth::Healthy;

        balancer.update_node_metrics(node1.id, metrics1).await;
        balancer.update_node_metrics(node2.id, metrics2).await;

        let selected = balancer.select_node(&nodes).await.unwrap();
        // Should work regardless of which strategy is chosen
        assert!(selected == node1.id || selected == node2.id);
    }
}
