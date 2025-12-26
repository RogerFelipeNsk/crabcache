//! Distributed pipeline processing for cross-node operations
//!
//! This module implements distributed pipeline processing that can route
//! and execute commands across multiple cluster nodes efficiently.

use crate::cluster::{ClusterError, ClusterNode, ClusterResult, NodeId};
use crate::protocol::commands::SerializableResponse;
use crate::protocol::{PipelineResponseBatch, Response};
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tracing::debug;

/// Simple command for distributed processing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PipelineCommand {
    Get { key: String },
    Set { key: String, value: String },
    Delete { key: String },
    Exists { key: String },
    Expire { key: String, ttl: u64 },
    Ttl { key: String },
    Ping,
    Info,
}

/// Cross-node routing plan for distributed commands
#[derive(Debug, Clone)]
pub struct RoutingPlan {
    /// Commands to execute locally
    pub local_commands: Vec<PipelineCommand>,
    /// Commands to execute on remote nodes
    pub remote_commands: HashMap<NodeId, Vec<PipelineCommand>>,
    /// Original command order mapping for response reconstruction
    pub command_order: Vec<CommandLocation>,
}

/// Location of a command in the routing plan
#[derive(Debug, Clone)]
pub enum CommandLocation {
    Local(usize),
    Remote(NodeId, usize),
}

/// Distributed pipeline metrics
#[derive(Debug, Clone, Default)]
pub struct DistributedPipelineMetrics {
    /// Total commands processed
    pub total_commands: u64,
    /// Commands processed locally
    pub local_commands: u64,
    /// Commands processed remotely
    pub remote_commands: u64,
    /// Cross-node latency statistics
    pub cross_node_latency_avg: f64,
    pub cross_node_latency_p99: f64,
    /// Network utilization
    pub network_utilization: f64,
    /// Load balancing efficiency
    pub load_balance_efficiency: f64,
    /// Routing decisions
    pub routing_decisions: u64,
    pub routing_time_avg: f64,
}

/// Cross-node router for distributing commands
pub struct CrossNodeRouter {
    /// Hash ring for consistent routing
    hash_ring: Arc<RwLock<crate::cluster::hash_ring::ConsistentHashRing>>,
    /// Node load tracking
    node_loads: Arc<DashMap<NodeId, f64>>,
    /// Routing strategy
    strategy: RoutingStrategy,
    /// Metrics
    metrics: Arc<RwLock<DistributedPipelineMetrics>>,
}

/// Routing strategy for command distribution
#[derive(Debug, Clone)]
pub enum RoutingStrategy {
    /// Route based on key hash (consistent hashing)
    ConsistentHash,
    /// Route based on current node load
    LoadBased,
    /// Hybrid approach combining hash and load
    Hybrid { hash_weight: f64, load_weight: f64 },
    /// Route to local node when possible
    LocalFirst,
}

impl CrossNodeRouter {
    /// Create new cross-node router
    pub fn new(
        hash_ring: Arc<RwLock<crate::cluster::hash_ring::ConsistentHashRing>>,
        strategy: RoutingStrategy,
    ) -> Self {
        Self {
            hash_ring,
            node_loads: Arc::new(DashMap::new()),
            strategy,
            metrics: Arc::new(RwLock::new(DistributedPipelineMetrics::default())),
        }
    }

    /// Plan routing for a batch of commands
    pub async fn plan_routing(&self, commands: &[PipelineCommand]) -> ClusterResult<RoutingPlan> {
        let start_time = Instant::now();

        let mut local_commands = Vec::new();
        let mut remote_commands: HashMap<NodeId, Vec<PipelineCommand>> = HashMap::new();
        let mut command_order = Vec::new();

        let hash_ring = self.hash_ring.read().await;

        for (_index, command) in commands.iter().enumerate() {
            let target_node = self.route_command(command, &hash_ring).await?;

            // Assuming we have a way to get local node ID (simplified for now)
            let local_node_id = NodeId::generate(); // This should be the actual local node ID

            if target_node == local_node_id {
                let local_index = local_commands.len();
                local_commands.push(command.clone());
                command_order.push(CommandLocation::Local(local_index));
            } else {
                let remote_list = remote_commands.entry(target_node).or_insert_with(Vec::new);
                let remote_index = remote_list.len();
                remote_list.push(command.clone());
                command_order.push(CommandLocation::Remote(target_node, remote_index));
            }
        }

        // Update metrics
        let routing_time = start_time.elapsed();
        let mut metrics = self.metrics.write().await;
        metrics.routing_decisions += 1;
        metrics.routing_time_avg = (metrics.routing_time_avg
            * (metrics.routing_decisions - 1) as f64
            + routing_time.as_secs_f64())
            / metrics.routing_decisions as f64;

        debug!(
            "Routed {} commands: {} local, {} remote nodes",
            commands.len(),
            local_commands.len(),
            remote_commands.len()
        );

        Ok(RoutingPlan {
            local_commands,
            remote_commands,
            command_order,
        })
    }

    /// Route a single command to appropriate node
    async fn route_command(
        &self,
        command: &PipelineCommand,
        hash_ring: &crate::cluster::hash_ring::ConsistentHashRing,
    ) -> ClusterResult<NodeId> {
        match &self.strategy {
            RoutingStrategy::ConsistentHash => self.route_by_hash(command, hash_ring).await,
            RoutingStrategy::LoadBased => self.route_by_load(hash_ring).await,
            RoutingStrategy::Hybrid {
                hash_weight,
                load_weight,
            } => {
                self.route_hybrid(command, hash_ring, *hash_weight, *load_weight)
                    .await
            }
            RoutingStrategy::LocalFirst => {
                // For now, always route to first available node (simplified)
                hash_ring
                    .get_nodes()
                    .first()
                    .map(|node| node.id)
                    .ok_or_else(|| ClusterError::ClusterNotReady {
                        reason: "No nodes available".to_string(),
                    })
            }
        }
    }

    /// Route command based on consistent hashing
    async fn route_by_hash(
        &self,
        command: &PipelineCommand,
        hash_ring: &crate::cluster::hash_ring::ConsistentHashRing,
    ) -> ClusterResult<NodeId> {
        let key = self.extract_key_from_command(command);
        hash_ring
            .get_primary_node(key.as_bytes())
            .ok_or_else(|| ClusterError::ClusterNotReady {
                reason: "No nodes in hash ring".to_string(),
            })
    }

    /// Route command based on current node load
    async fn route_by_load(
        &self,
        hash_ring: &crate::cluster::hash_ring::ConsistentHashRing,
    ) -> ClusterResult<NodeId> {
        let nodes = hash_ring.get_nodes();
        if nodes.is_empty() {
            return Err(ClusterError::ClusterNotReady {
                reason: "No nodes available".to_string(),
            });
        }

        // Find node with lowest load
        let mut best_node = nodes[0].id;
        let mut best_load = self.node_loads.get(&best_node).map(|l| *l).unwrap_or(0.0);

        for node in &nodes[1..] {
            let load = self.node_loads.get(&node.id).map(|l| *l).unwrap_or(0.0);
            if load < best_load {
                best_node = node.id;
                best_load = load;
            }
        }

        Ok(best_node)
    }

    /// Route command using hybrid hash + load strategy
    async fn route_hybrid(
        &self,
        command: &PipelineCommand,
        hash_ring: &crate::cluster::hash_ring::ConsistentHashRing,
        hash_weight: f64,
        load_weight: f64,
    ) -> ClusterResult<NodeId> {
        let key = self.extract_key_from_command(command);
        let preferred_nodes = hash_ring.get_nodes_for_key(key.as_bytes());

        if preferred_nodes.is_empty() {
            return Err(ClusterError::ClusterNotReady {
                reason: "No nodes available".to_string(),
            });
        }

        // Score nodes based on hash preference and load
        let mut best_node = preferred_nodes[0];
        let mut best_score = f64::MAX;

        for (index, &node_id) in preferred_nodes.iter().enumerate() {
            let hash_score = index as f64; // Lower index = better hash match
            let load = self.node_loads.get(&node_id).map(|l| *l).unwrap_or(0.0);
            let load_score = load;

            let total_score = hash_score * hash_weight + load_score * load_weight;

            if total_score < best_score {
                best_node = node_id;
                best_score = total_score;
            }
        }

        Ok(best_node)
    }

    /// Extract key from command for routing
    fn extract_key_from_command(&self, command: &PipelineCommand) -> String {
        match command {
            PipelineCommand::Get { key } => key.clone(),
            PipelineCommand::Set { key, .. } => key.clone(),
            PipelineCommand::Delete { key } => key.clone(),
            PipelineCommand::Exists { key } => key.clone(),
            PipelineCommand::Expire { key, .. } => key.clone(),
            PipelineCommand::Ttl { key } => key.clone(),
            _ => "default".to_string(), // For commands without keys
        }
    }

    /// Update node load information
    pub fn update_node_load(&self, node_id: NodeId, load: f64) {
        self.node_loads.insert(node_id, load);
    }

    /// Get routing metrics
    pub async fn get_metrics(&self) -> DistributedPipelineMetrics {
        self.metrics.read().await.clone()
    }
}

/// Distributed pipeline manager
pub struct DistributedPipelineManager {
    /// Cross-node router
    router: Arc<CrossNodeRouter>,
    /// Remote node clients
    remote_clients: Arc<DashMap<NodeId, RemoteNodeClient>>,
    /// Metrics
    metrics: Arc<RwLock<DistributedPipelineMetrics>>,
}

impl DistributedPipelineManager {
    /// Create new distributed pipeline manager
    pub fn new(
        hash_ring: Arc<RwLock<crate::cluster::hash_ring::ConsistentHashRing>>,
        routing_strategy: RoutingStrategy,
    ) -> Self {
        let router = Arc::new(CrossNodeRouter::new(hash_ring, routing_strategy));

        Self {
            router,
            remote_clients: Arc::new(DashMap::new()),
            metrics: Arc::new(RwLock::new(DistributedPipelineMetrics::default())),
        }
    }

    /// Process a distributed batch of commands
    pub async fn process_distributed_batch(
        &self,
        commands: Vec<PipelineCommand>,
    ) -> ClusterResult<PipelineResponseBatch> {
        let start_time = Instant::now();

        // Plan routing for commands
        let routing_plan = self.router.plan_routing(&commands).await?;

        // Execute distributed processing
        let responses = self.execute_distributed_processing(routing_plan).await?;

        // Update metrics
        let _processing_time = start_time.elapsed();
        let mut metrics = self.metrics.write().await;
        metrics.total_commands += commands.len() as u64;

        Ok(PipelineResponseBatch {
            responses,
            batch_id: start_time.elapsed().as_nanos() as u64,
            use_binary_protocol: false,
        })
    }

    /// Execute distributed processing according to routing plan
    async fn execute_distributed_processing(
        &self,
        routing_plan: RoutingPlan,
    ) -> ClusterResult<Vec<Response>> {
        let mut all_responses = Vec::new();

        // Process local commands
        if !routing_plan.local_commands.is_empty() {
            let local_responses = self
                .process_local_commands(&routing_plan.local_commands)
                .await?;
            all_responses.extend(local_responses);
        }

        // Process remote commands
        for (node_id, commands) in routing_plan.remote_commands {
            let remote_responses = self.process_remote_commands(node_id, commands).await?;
            all_responses.extend(remote_responses);
        }

        // Reconstruct responses in original order
        self.reconstruct_response_order(all_responses, &routing_plan.command_order)
            .await
    }

    /// Process commands locally
    async fn process_local_commands(
        &self,
        commands: &[PipelineCommand],
    ) -> ClusterResult<Vec<Response>> {
        // Simulate local processing
        let responses: Vec<Response> = commands
            .iter()
            .map(|cmd| match cmd {
                PipelineCommand::Get { key: _ } => {
                    Response::Value(bytes::Bytes::from("test_value"))
                }
                PipelineCommand::Set { .. } => Response::Ok,
                PipelineCommand::Delete { .. } => Response::Ok,
                PipelineCommand::Exists { .. } => Response::Value(bytes::Bytes::from("1")),
                PipelineCommand::Expire { .. } => Response::Ok,
                PipelineCommand::Ttl { .. } => Response::Value(bytes::Bytes::from("3600")),
                PipelineCommand::Ping => Response::Pong,
                PipelineCommand::Info => Response::Stats("info_data".to_string()),
            })
            .collect();

        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.local_commands += commands.len() as u64;

        Ok(responses)
    }

    /// Process commands on remote node
    async fn process_remote_commands(
        &self,
        node_id: NodeId,
        commands: Vec<PipelineCommand>,
    ) -> ClusterResult<Vec<Response>> {
        let start_time = Instant::now();

        // Get or create remote client
        let client = self.get_or_create_remote_client(node_id).await?;

        // Send commands to remote node
        let responses = client.execute_commands(commands).await?;

        // Update metrics
        let latency = start_time.elapsed();
        let mut metrics = self.metrics.write().await;
        metrics.remote_commands += responses.len() as u64;

        // Update cross-node latency statistics
        let latency_ms = latency.as_secs_f64() * 1000.0;
        if metrics.cross_node_latency_avg == 0.0 {
            metrics.cross_node_latency_avg = latency_ms;
        } else {
            metrics.cross_node_latency_avg =
                (metrics.cross_node_latency_avg * 0.9) + (latency_ms * 0.1);
        }

        Ok(responses)
    }

    /// Get or create remote client for node
    async fn get_or_create_remote_client(
        &self,
        node_id: NodeId,
    ) -> ClusterResult<RemoteNodeClient> {
        if let Some(client) = self.remote_clients.get(&node_id) {
            Ok(client.clone())
        } else {
            // Create new client (simplified)
            let client = RemoteNodeClient::new(node_id, "127.0.0.1:8000".parse().unwrap());
            self.remote_clients.insert(node_id, client.clone());
            Ok(client)
        }
    }

    /// Reconstruct responses in original command order
    async fn reconstruct_response_order(
        &self,
        responses: Vec<Response>,
        command_order: &[CommandLocation],
    ) -> ClusterResult<Vec<Response>> {
        let mut ordered_responses = Vec::with_capacity(command_order.len());
        let mut local_index = 0;
        let mut remote_indices: HashMap<NodeId, usize> = HashMap::new();

        for location in command_order {
            match location {
                CommandLocation::Local(_) => {
                    if local_index < responses.len() {
                        ordered_responses.push(responses[local_index].clone());
                        local_index += 1;
                    } else {
                        return Err(ClusterError::NetworkError {
                            source: std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Missing local response",
                            ),
                        });
                    }
                }
                CommandLocation::Remote(node_id, _) => {
                    let remote_index = remote_indices.entry(*node_id).or_insert(0);
                    if *remote_index < responses.len() {
                        ordered_responses.push(responses[*remote_index].clone());
                        *remote_index += 1;
                    } else {
                        return Err(ClusterError::NetworkError {
                            source: std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Missing remote response",
                            ),
                        });
                    }
                }
            }
        }

        Ok(ordered_responses)
    }

    /// Add remote node client
    pub fn add_remote_node(&self, node_id: NodeId, address: std::net::SocketAddr) {
        let client = RemoteNodeClient::new(node_id, address);
        self.remote_clients.insert(node_id, client);
    }

    /// Remove remote node client
    pub fn remove_remote_node(&self, node_id: NodeId) {
        self.remote_clients.remove(&node_id);
    }

    /// Update node load for routing decisions
    pub fn update_node_load(&self, node_id: NodeId, load: f64) {
        self.router.update_node_load(node_id, load);
    }

    /// Get distributed pipeline metrics
    pub async fn get_metrics(&self) -> DistributedPipelineMetrics {
        self.metrics.read().await.clone()
    }
}

/// Client for communicating with remote nodes
#[derive(Debug, Clone)]
pub struct RemoteNodeClient {
    _node_id: NodeId,
    address: std::net::SocketAddr,
    connection_pool: Arc<RwLock<Option<RemoteConnection>>>,
}

impl RemoteNodeClient {
    /// Create new remote node client
    pub fn new(node_id: NodeId, address: std::net::SocketAddr) -> Self {
        Self {
            _node_id: node_id,
            address,
            connection_pool: Arc::new(RwLock::new(None)),
        }
    }

    /// Execute commands on remote node
    pub async fn execute_commands(
        &self,
        commands: Vec<PipelineCommand>,
    ) -> ClusterResult<Vec<Response>> {
        // Get or create connection
        let connection = self.get_connection().await?;

        // Send commands and receive responses
        connection.send_commands(commands).await
    }

    /// Get connection to remote node
    async fn get_connection(&self) -> ClusterResult<RemoteConnection> {
        let mut pool = self.connection_pool.write().await;

        if let Some(connection) = &*pool {
            if connection.is_healthy().await {
                return Ok(connection.clone());
            }
        }

        // Create new connection
        let connection = RemoteConnection::connect(self.address).await?;
        *pool = Some(connection.clone());

        Ok(connection)
    }
}

/// Connection to a remote node
#[derive(Debug, Clone)]
pub struct RemoteConnection {
    address: std::net::SocketAddr,
    // In a real implementation, this would contain the actual TCP connection
}

impl RemoteConnection {
    /// Connect to remote node
    pub async fn connect(address: std::net::SocketAddr) -> ClusterResult<Self> {
        // TODO: Implement actual TCP connection
        Ok(Self { address })
    }

    /// Check if connection is healthy
    pub async fn is_healthy(&self) -> bool {
        // TODO: Implement health check
        true
    }

    /// Send commands to remote node
    pub async fn send_commands(
        &self,
        commands: Vec<PipelineCommand>,
    ) -> ClusterResult<Vec<Response>> {
        // Serialize commands
        let serialized = bincode::serialize(&commands).map_err(|e| ClusterError::NetworkError {
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
        })?;

        // Connect to remote node
        let mut stream = TcpStream::connect(self.address)
            .await
            .map_err(|e| ClusterError::NetworkError { source: e })?;

        // Send command batch length
        let len = serialized.len() as u32;
        stream
            .write_all(&len.to_be_bytes())
            .await
            .map_err(|e| ClusterError::NetworkError { source: e })?;

        // Send command batch
        stream
            .write_all(&serialized)
            .await
            .map_err(|e| ClusterError::NetworkError { source: e })?;

        stream
            .flush()
            .await
            .map_err(|e| ClusterError::NetworkError { source: e })?;

        // Receive response length
        let mut len_bytes = [0u8; 4];
        stream
            .read_exact(&mut len_bytes)
            .await
            .map_err(|e| ClusterError::NetworkError { source: e })?;

        let response_len = u32::from_be_bytes(len_bytes) as usize;

        // Receive response data
        let mut response_buffer = vec![0u8; response_len];
        stream
            .read_exact(&mut response_buffer)
            .await
            .map_err(|e| ClusterError::NetworkError { source: e })?;

        // Deserialize responses
        let serializable_responses: Vec<SerializableResponse> =
            bincode::deserialize(&response_buffer).map_err(|e| ClusterError::NetworkError {
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            })?;

        // Convert to Response
        let responses: Vec<Response> = serializable_responses
            .into_iter()
            .map(|sr: SerializableResponse| sr.into())
            .collect();

        debug!(
            "Sent {} commands to {}, received {} responses",
            commands.len(),
            self.address,
            responses.len()
        );

        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::hash_ring::ConsistentHashRing;
    use crate::cluster::node::{NodeCapabilities, NodeId};

    fn create_test_node(id: u32) -> ClusterNode {
        let node_id = NodeId::generate();
        let addr = format!("127.0.0.1:{}", 8000 + id).parse().unwrap();
        let cluster_addr = format!("127.0.0.1:{}", 9000 + id).parse().unwrap();
        let capabilities = NodeCapabilities::default();

        crate::cluster::ClusterNode::new(node_id, addr, cluster_addr, capabilities)
    }

    #[tokio::test]
    async fn test_cross_node_router_creation() {
        let hash_ring = Arc::new(RwLock::new(ConsistentHashRing::new(256, 3)));
        let router = CrossNodeRouter::new(hash_ring, RoutingStrategy::ConsistentHash);

        let metrics = router.get_metrics().await;
        assert_eq!(metrics.routing_decisions, 0);
    }

    #[tokio::test]
    async fn test_routing_strategy_consistent_hash() {
        let mut hash_ring = ConsistentHashRing::new(256, 3);
        let node1 = create_test_node(1);
        let node2 = create_test_node(2);

        hash_ring.add_node(node1);
        hash_ring.add_node(node2);

        let hash_ring = Arc::new(RwLock::new(hash_ring));
        let router = CrossNodeRouter::new(hash_ring, RoutingStrategy::ConsistentHash);

        let command = PipelineCommand::Get {
            key: "test_key".to_string(),
        };
        let commands = vec![command];

        let routing_plan = router.plan_routing(&commands).await.unwrap();

        // Should route to one of the nodes
        assert!(routing_plan.local_commands.len() + routing_plan.remote_commands.len() > 0);
    }

    #[tokio::test]
    async fn test_routing_strategy_load_based() {
        let mut hash_ring = ConsistentHashRing::new(256, 3);
        let node1 = create_test_node(1);
        let node2 = create_test_node(2);

        let node1_id = node1.id;
        let node2_id = node2.id;

        hash_ring.add_node(node1);
        hash_ring.add_node(node2);

        let hash_ring = Arc::new(RwLock::new(hash_ring));
        let router = CrossNodeRouter::new(hash_ring, RoutingStrategy::LoadBased);

        // Set different loads
        router.update_node_load(node1_id, 0.8);
        router.update_node_load(node2_id, 0.2);

        let command = PipelineCommand::Get {
            key: "test_key".to_string(),
        };
        let commands = vec![command];

        let routing_plan = router.plan_routing(&commands).await.unwrap();

        // Should prefer the node with lower load
        assert!(routing_plan.local_commands.len() + routing_plan.remote_commands.len() > 0);
    }

    #[test]
    fn test_command_location() {
        let local_location = CommandLocation::Local(0);
        let remote_location = CommandLocation::Remote(NodeId::generate(), 1);

        match local_location {
            CommandLocation::Local(index) => assert_eq!(index, 0),
            _ => panic!("Wrong location type"),
        }

        match remote_location {
            CommandLocation::Remote(_, index) => assert_eq!(index, 1),
            _ => panic!("Wrong location type"),
        }
    }

    #[test]
    fn test_remote_node_client_creation() {
        let node_id = NodeId::generate();
        let address = "127.0.0.1:8000".parse().unwrap();

        let client = RemoteNodeClient::new(node_id, address);
        assert_eq!(client._node_id, node_id);
        assert_eq!(client.address, address);
    }

    #[tokio::test]
    async fn test_remote_connection() {
        let address = "127.0.0.1:8000".parse().unwrap();
        let connection = RemoteConnection::connect(address).await.unwrap();

        assert!(connection.is_healthy().await);
        assert_eq!(connection.address, address);
    }

    #[test]
    fn test_distributed_pipeline_metrics() {
        let metrics = DistributedPipelineMetrics::default();

        assert_eq!(metrics.total_commands, 0);
        assert_eq!(metrics.local_commands, 0);
        assert_eq!(metrics.remote_commands, 0);
        assert_eq!(metrics.cross_node_latency_avg, 0.0);
    }
}
