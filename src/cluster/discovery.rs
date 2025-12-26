//! Service discovery and cluster management
//! 
//! This module implements node discovery, cluster membership management,
//! and health monitoring for distributed CrabCache clusters.

use crate::cluster::{ClusterNode, NodeId, NodeStatus, ClusterResult, ClusterError, ClusterConfig, ClusterMetrics};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock as AsyncRwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Discovery message types for inter-node communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoveryMessage {
    /// Node announces itself to the cluster
    NodeAnnouncement {
        node: ClusterNode,
        cluster_name: String,
    },
    /// Node requests cluster membership
    JoinRequest {
        node: ClusterNode,
        cluster_name: String,
    },
    /// Response to join request
    JoinResponse {
        accepted: bool,
        reason: Option<String>,
        cluster_nodes: Vec<ClusterNode>,
    },
    /// Periodic heartbeat
    Heartbeat {
        node_id: NodeId,
        stats: crate::cluster::node::NodeStats,
        timestamp: u64,
    },
    /// Node leaving cluster gracefully
    LeaveNotification {
        node_id: NodeId,
        reason: String,
    },
    /// Request for cluster status
    StatusRequest,
    /// Response with cluster status
    StatusResponse {
        cluster_metrics: ClusterMetrics,
        nodes: Vec<ClusterNode>,
    },
}

/// Service discovery implementation
pub struct ServiceDiscovery {
    config: ClusterConfig,
    local_node: Arc<AsyncRwLock<ClusterNode>>,
    discovered_nodes: Arc<DashMap<NodeId, ClusterNode>>,
    message_sender: mpsc::UnboundedSender<DiscoveryMessage>,
    message_receiver: Arc<AsyncRwLock<Option<mpsc::UnboundedReceiver<DiscoveryMessage>>>>,
    shutdown_sender: broadcast::Sender<()>,
}

impl ServiceDiscovery {
    /// Create new service discovery instance
    pub fn new(config: ClusterConfig, local_node: ClusterNode) -> Self {
        let (message_sender, message_receiver) = mpsc::unbounded_channel();
        let (shutdown_sender, _) = broadcast::channel(1);
        
        Self {
            config,
            local_node: Arc::new(AsyncRwLock::new(local_node)),
            discovered_nodes: Arc::new(DashMap::new()),
            message_sender,
            message_receiver: Arc::new(AsyncRwLock::new(Some(message_receiver))),
            shutdown_sender,
        }
    }
    
    /// Start service discovery
    pub async fn start(&self) -> ClusterResult<()> {
        info!("Starting service discovery on {}", self.config.bind_address);
        
        // Start TCP listener for incoming discovery messages
        let listener = TcpListener::bind(self.config.bind_address)
            .await
            .map_err(|e| ClusterError::NetworkError { source: e })?;
        
        // Start message processing task
        self.start_message_processor().await;
        
        // Start heartbeat task
        self.start_heartbeat_task().await;
        
        // Start connection handler
        self.start_connection_handler(listener).await;
        
        Ok(())
    }
    
    /// Discover nodes in the cluster
    pub async fn discover_nodes(&self, seed_nodes: Vec<SocketAddr>) -> ClusterResult<Vec<ClusterNode>> {
        info!("Discovering nodes from {} seed nodes", seed_nodes.len());
        
        let mut discovered = Vec::new();
        
        for seed_addr in seed_nodes {
            match self.contact_seed_node(seed_addr).await {
                Ok(nodes) => {
                    for node in nodes {
                        if !discovered.iter().any(|n: &ClusterNode| n.id == node.id) {
                            discovered.push(node.clone());
                            self.discovered_nodes.insert(node.id, node);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to contact seed node {}: {}", seed_addr, e);
                }
            }
        }
        
        info!("Discovered {} nodes", discovered.len());
        Ok(discovered)
    }
    
    /// Contact a seed node to get cluster information
    async fn contact_seed_node(&self, addr: SocketAddr) -> ClusterResult<Vec<ClusterNode>> {
        debug!("Contacting seed node at {}", addr);
        
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|e| ClusterError::NetworkError { source: e })?;
        
        // Send status request
        let message = DiscoveryMessage::StatusRequest;
        self.send_message_to_stream(&mut stream, &message).await?;
        
        // Receive response
        let response = self.receive_message_from_stream(&mut stream).await?;
        
        match response {
            DiscoveryMessage::StatusResponse { nodes, .. } => Ok(nodes),
            _ => Err(ClusterError::NetworkError { 
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData, 
                    "Unexpected response from seed node"
                )
            }),
        }
    }
    
    /// Start message processing task
    async fn start_message_processor(&self) {
        let mut receiver = self.message_receiver.write().await.take()
            .expect("Message receiver should be available");
        
        let discovered_nodes = self.discovered_nodes.clone();
        let local_node = self.local_node.clone();
        let config = self.config.clone();
        let mut shutdown_receiver = self.shutdown_sender.subscribe();
        
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    message = receiver.recv() => {
                        match message {
                            Some(msg) => {
                                if let Err(e) = Self::process_discovery_message(
                                    &msg, 
                                    &discovered_nodes, 
                                    &local_node,
                                    &config
                                ).await {
                                    error!("Error processing discovery message: {}", e);
                                }
                            }
                            None => break,
                        }
                    }
                    _ = shutdown_receiver.recv() => {
                        info!("Shutting down message processor");
                        break;
                    }
                }
            }
        });
    }
    
    /// Process incoming discovery message
    async fn process_discovery_message(
        message: &DiscoveryMessage,
        discovered_nodes: &DashMap<NodeId, ClusterNode>,
        local_node: &AsyncRwLock<ClusterNode>,
        config: &ClusterConfig,
    ) -> ClusterResult<()> {
        match message {
            DiscoveryMessage::NodeAnnouncement { node, cluster_name } => {
                if cluster_name == &config.cluster_name {
                    info!("Node {} announced itself to cluster", node.id);
                    discovered_nodes.insert(node.id, node.clone());
                } else {
                    warn!("Ignoring announcement from different cluster: {}", cluster_name);
                }
            }
            
            DiscoveryMessage::JoinRequest { node, cluster_name } => {
                if cluster_name == &config.cluster_name {
                    info!("Node {} requesting to join cluster", node.id);
                    
                    // Accept join request (in real implementation, this might involve consensus)
                    discovered_nodes.insert(node.id, node.clone());
                    
                    // TODO: Send JoinResponse
                } else {
                    warn!("Rejecting join request from different cluster: {}", cluster_name);
                }
            }
            
            DiscoveryMessage::Heartbeat { node_id, stats, .. } => {
                if let Some(mut node) = discovered_nodes.get_mut(node_id) {
                    node.update_stats(stats.clone());
                    debug!("Updated heartbeat for node {}", node_id);
                } else {
                    warn!("Received heartbeat from unknown node: {}", node_id);
                }
            }
            
            DiscoveryMessage::LeaveNotification { node_id, reason } => {
                if let Some((_, mut node)) = discovered_nodes.remove(node_id) {
                    node.status = NodeStatus::Leaving;
                    info!("Node {} leaving cluster: {}", node_id, reason);
                }
            }
            
            DiscoveryMessage::StatusRequest => {
                debug!("Received status request");
                // TODO: Send StatusResponse
            }
            
            DiscoveryMessage::StatusResponse { .. } => {
                debug!("Received status response");
                // Handled in discover_nodes method
            }
            
            _ => {
                debug!("Unhandled discovery message: {:?}", message);
            }
        }
        
        Ok(())
    }
    
    /// Start heartbeat task
    async fn start_heartbeat_task(&self) {
        let local_node = self.local_node.clone();
        let discovered_nodes = self.discovered_nodes.clone();
        let heartbeat_interval = Duration::from_millis(self.config.heartbeat_interval_ms);
        let mut shutdown_receiver = self.shutdown_sender.subscribe();
        
        tokio::spawn(async move {
            let mut interval = interval(heartbeat_interval);
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let node = local_node.read().await;
                        let _heartbeat = DiscoveryMessage::Heartbeat {
                            node_id: node.id,
                            stats: node.stats.clone(),
                            timestamp: chrono::Utc::now().timestamp_millis() as u64,
                        };
                        
                        // Send heartbeat to all known nodes
                        for node_entry in discovered_nodes.iter() {
                            let target_node = node_entry.value();
                            if target_node.id != node.id {
                                // TODO: Send heartbeat to target_node
                                debug!("Sending heartbeat to node {}", target_node.id);
                            }
                        }
                    }
                    _ = shutdown_receiver.recv() => {
                        info!("Shutting down heartbeat task");
                        break;
                    }
                }
            }
        });
    }
    
    /// Start connection handler
    async fn start_connection_handler(&self, listener: TcpListener) {
        let message_sender = self.message_sender.clone();
        let mut shutdown_receiver = self.shutdown_sender.subscribe();
        
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((stream, addr)) => {
                                debug!("Accepted connection from {}", addr);
                                
                                let sender = message_sender.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = Self::handle_connection(stream, sender).await {
                                        error!("Error handling connection from {}: {}", addr, e);
                                    }
                                });
                            }
                            Err(e) => {
                                error!("Error accepting connection: {}", e);
                            }
                        }
                    }
                    _ = shutdown_receiver.recv() => {
                        info!("Shutting down connection handler");
                        break;
                    }
                }
            }
        });
    }
    
    /// Handle incoming connection
    async fn handle_connection(
        mut stream: TcpStream,
        message_sender: mpsc::UnboundedSender<DiscoveryMessage>,
    ) -> ClusterResult<()> {
        // Receive message from stream
        let message = Self::receive_message_from_stream_static(&mut stream).await?;
        
        // Process message and send response if needed
        match &message {
            DiscoveryMessage::StatusRequest => {
                // Send status response
                let response = DiscoveryMessage::StatusResponse {
                    cluster_metrics: ClusterMetrics::default(),
                    nodes: vec![], // TODO: Get actual cluster nodes
                };
                
                // Send response length first
                let serialized = bincode::serialize(&response)
                    .map_err(|e| ClusterError::NetworkError { 
                        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
                    })?;
                
                let len = serialized.len() as u32;
                stream.write_all(&len.to_be_bytes()).await
                    .map_err(|e| ClusterError::NetworkError { source: e })?;
                
                stream.write_all(&serialized).await
                    .map_err(|e| ClusterError::NetworkError { source: e })?;
                
                stream.flush().await
                    .map_err(|e| ClusterError::NetworkError { source: e })?;
            }
            _ => {
                // Forward to message processor
                message_sender.send(message)
                    .map_err(|_| ClusterError::NetworkError { 
                        source: std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe, 
                            "Message channel closed"
                        )
                    })?;
            }
        }
        
        Ok(())
    }
    
    /// Send message to TCP stream
    async fn send_message_to_stream(
        &self,
        stream: &mut TcpStream,
        message: &DiscoveryMessage,
    ) -> ClusterResult<()> {
        let serialized = bincode::serialize(message)
            .map_err(|e| ClusterError::NetworkError { 
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
            })?;
        
        // Send message length first (4 bytes)
        let len = serialized.len() as u32;
        stream.write_all(&len.to_be_bytes()).await
            .map_err(|e| ClusterError::NetworkError { source: e })?;
        
        // Send message data
        stream.write_all(&serialized).await
            .map_err(|e| ClusterError::NetworkError { source: e })?;
        
        stream.flush().await
            .map_err(|e| ClusterError::NetworkError { source: e })?;
        
        debug!("Sent {} bytes to stream", serialized.len());
        Ok(())
    }
    
    /// Receive message from TCP stream
    async fn receive_message_from_stream(&self, stream: &mut TcpStream) -> ClusterResult<DiscoveryMessage> {
        // Read message length first (4 bytes)
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes).await
            .map_err(|e| ClusterError::NetworkError { source: e })?;
        
        let len = u32::from_be_bytes(len_bytes) as usize;
        
        // Read message data
        let mut buffer = vec![0u8; len];
        stream.read_exact(&mut buffer).await
            .map_err(|e| ClusterError::NetworkError { source: e })?;
        
        // Deserialize message
        let message = bincode::deserialize(&buffer)
            .map_err(|e| ClusterError::NetworkError { 
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
            })?;
        
        debug!("Received {} bytes from stream", len);
        Ok(message)
    }
    
    /// Static version of receive_message_from_stream
    async fn receive_message_from_stream_static(stream: &mut TcpStream) -> ClusterResult<DiscoveryMessage> {
        // Read message length first (4 bytes)
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes).await
            .map_err(|e| ClusterError::NetworkError { source: e })?;
        
        let len = u32::from_be_bytes(len_bytes) as usize;
        
        // Read message data
        let mut buffer = vec![0u8; len];
        stream.read_exact(&mut buffer).await
            .map_err(|e| ClusterError::NetworkError { source: e })?;
        
        // Deserialize message
        let message = bincode::deserialize(&buffer)
            .map_err(|e| ClusterError::NetworkError { 
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
            })?;
        
        debug!("Received {} bytes from stream", len);
        Ok(message)
    }
    
    /// Get all discovered nodes
    pub fn get_discovered_nodes(&self) -> Vec<ClusterNode> {
        self.discovered_nodes.iter().map(|entry| entry.value().clone()).collect()
    }
    
    /// Get node by ID
    pub fn get_node(&self, node_id: NodeId) -> Option<ClusterNode> {
        self.discovered_nodes.get(&node_id).map(|entry| entry.value().clone())
    }
    
    /// Update local node stats
    pub async fn update_local_node_stats(&self, stats: crate::cluster::node::NodeStats) {
        let mut node = self.local_node.write().await;
        node.update_stats(stats);
    }
    
    /// Shutdown service discovery
    pub async fn shutdown(&self) -> ClusterResult<()> {
        info!("Shutting down service discovery");
        
        // Send shutdown signal
        let _ = self.shutdown_sender.send(());
        
        // Send leave notification
        let local_node = self.local_node.read().await;
        let _leave_message = DiscoveryMessage::LeaveNotification {
            node_id: local_node.id,
            reason: "Graceful shutdown".to_string(),
        };
        
        // TODO: Send leave message to all nodes
        debug!("Would send leave notification for node {}", local_node.id);
        
        Ok(())
    }
}

/// Cluster manager that coordinates service discovery and cluster operations
pub struct ClusterManager {
    _config: ClusterConfig,
    local_node: Arc<AsyncRwLock<ClusterNode>>,
    discovery: Arc<ServiceDiscovery>,
    cluster_metrics: Arc<RwLock<ClusterMetrics>>,
    failure_detector: Arc<FailureDetector>,
}

impl ClusterManager {
    /// Create new cluster manager
    pub fn new(config: ClusterConfig, local_node: ClusterNode) -> Self {
        let local_node = Arc::new(AsyncRwLock::new(local_node));
        let discovery = Arc::new(ServiceDiscovery::new(config.clone(), {
            // Create a clone of the node for the discovery service
            let node_guard = local_node.try_read().unwrap();
            node_guard.clone()
        }));
        
        Self {
            _config: config.clone(),
            local_node,
            discovery,
            cluster_metrics: Arc::new(RwLock::new(ClusterMetrics::default())),
            failure_detector: Arc::new(FailureDetector::new(config)),
        }
    }
    
    /// Start cluster manager
    pub async fn start(&self) -> ClusterResult<()> {
        info!("Starting cluster manager");
        
        // Start service discovery
        self.discovery.start().await?;
        
        // Start failure detector
        self.failure_detector.start().await?;
        
        // Start metrics collection
        self.start_metrics_collection().await;
        
        Ok(())
    }
    
    /// Join cluster using seed nodes
    pub async fn join_cluster(&self, seed_nodes: Vec<SocketAddr>) -> ClusterResult<()> {
        info!("Joining cluster with {} seed nodes", seed_nodes.len());
        
        // Discover existing nodes
        let discovered_nodes = self.discovery.discover_nodes(seed_nodes).await?;
        
        // Announce self to cluster
        self.announce_to_cluster(&discovered_nodes).await?;
        
        // Wait for cluster acceptance (simplified for now)
        tokio::time::sleep(Duration::from_millis(1000)).await;
        
        // Mark local node as active
        {
            let mut local_node = self.local_node.write().await;
            local_node.mark_active();
        }
        
        info!("Successfully joined cluster");
        Ok(())
    }
    
    /// Announce node to cluster
    async fn announce_to_cluster(&self, _nodes: &[ClusterNode]) -> ClusterResult<()> {
        let local_node = self.local_node.read().await;
        
        info!("Announcing node {} to cluster", local_node.id);
        
        // TODO: Send announcement to all nodes
        
        Ok(())
    }
    
    /// Leave cluster gracefully
    pub async fn leave_cluster(&self) -> ClusterResult<()> {
        info!("Leaving cluster gracefully");
        
        // Mark as leaving
        {
            let mut local_node = self.local_node.write().await;
            local_node.status = NodeStatus::Leaving;
        }
        
        // TODO: Migrate shards to other nodes
        
        // Shutdown discovery
        self.discovery.shutdown().await?;
        
        info!("Successfully left cluster");
        Ok(())
    }
    
    /// Get cluster metrics
    pub fn get_cluster_metrics(&self) -> ClusterMetrics {
        self.cluster_metrics.read().clone()
    }
    
    /// Get all cluster nodes
    pub fn get_cluster_nodes(&self) -> Vec<ClusterNode> {
        self.discovery.get_discovered_nodes()
    }
    
    /// Start metrics collection task
    async fn start_metrics_collection(&self) {
        let discovery = self.discovery.clone();
        let cluster_metrics = self.cluster_metrics.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            
            loop {
                interval.tick().await;
                
                let nodes = discovery.get_discovered_nodes();
                let mut metrics = ClusterMetrics::default();
                
                metrics.total_nodes = nodes.len() as u32;
                metrics.active_nodes = nodes.iter()
                    .filter(|n| n.status == NodeStatus::Active)
                    .count() as u32;
                metrics.failed_nodes = nodes.iter()
                    .filter(|n| n.status == NodeStatus::Failed)
                    .count() as u32;
                
                if !nodes.is_empty() {
                    let total_throughput: f64 = nodes.iter()
                        .map(|n| n.stats.current_ops_per_sec)
                        .sum();
                    metrics.cluster_throughput = total_throughput;
                    
                    let loads: Vec<f64> = nodes.iter()
                        .map(|n| n.load_factor())
                        .collect();
                    
                    if !loads.is_empty() {
                        metrics.avg_node_load = loads.iter().sum::<f64>() / loads.len() as f64;
                        metrics.max_node_load = loads.iter().fold(0.0, |a, &b| a.max(b));
                        metrics.min_node_load = loads.iter().fold(1.0, |a, &b| a.min(b));
                    }
                }
                
                *cluster_metrics.write() = metrics;
            }
        });
    }
}

/// Failure detector for monitoring node health
pub struct FailureDetector {
    _config: ClusterConfig,
    failure_threshold: Duration,
}

impl FailureDetector {
    /// Create new failure detector
    pub fn new(config: ClusterConfig) -> Self {
        Self {
            _config: config.clone(),
            failure_threshold: Duration::from_millis(config.election_timeout_ms * 2),
        }
    }
    
    /// Start failure detection
    pub async fn start(&self) -> ClusterResult<()> {
        info!("Starting failure detector");
        
        // TODO: Implement failure detection logic
        
        Ok(())
    }
    
    /// Check if node is considered failed
    pub fn is_node_failed(&self, node: &ClusterNode) -> bool {
        !node.is_alive(self.failure_threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::node::{NodeCapabilities, NodeId};
    use std::time::Instant;
    
    fn create_test_node(id: u32) -> ClusterNode {
        let node_id = NodeId::generate();
        let addr = format!("127.0.0.1:{}", 8000 + id).parse().unwrap();
        let cluster_addr = format!("127.0.0.1:{}", 9000 + id).parse().unwrap();
        let capabilities = NodeCapabilities::default();
        
        ClusterNode::new(node_id, addr, cluster_addr, capabilities)
    }
    
    #[tokio::test]
    async fn test_service_discovery_creation() {
        let config = ClusterConfig::default();
        let node = create_test_node(1);
        
        let discovery = ServiceDiscovery::new(config, node);
        assert_eq!(discovery.get_discovered_nodes().len(), 0);
    }
    
    #[tokio::test]
    async fn test_cluster_manager_creation() {
        let config = ClusterConfig::default();
        let node = create_test_node(1);
        
        let manager = ClusterManager::new(config, node);
        let metrics = manager.get_cluster_metrics();
        assert_eq!(metrics.total_nodes, 0);
    }
    
    #[tokio::test]
    async fn test_failure_detector() {
        let config = ClusterConfig::default();
        let detector = FailureDetector::new(config);
        
        let mut node = create_test_node(1);
        
        // Fresh node should not be failed
        assert!(!detector.is_node_failed(&node));
        
        // Old heartbeat should be considered failed
        node.last_heartbeat = Instant::now() - Duration::from_secs(30);
        assert!(detector.is_node_failed(&node));
    }
    
    #[test]
    fn test_discovery_message_serialization() {
        let node = create_test_node(1);
        let message = DiscoveryMessage::NodeAnnouncement {
            node,
            cluster_name: "test-cluster".to_string(),
        };
        
        let serialized = bincode::serialize(&message).unwrap();
        let deserialized: DiscoveryMessage = bincode::deserialize(&serialized).unwrap();
        
        match deserialized {
            DiscoveryMessage::NodeAnnouncement { cluster_name, .. } => {
                assert_eq!(cluster_name, "test-cluster");
            }
            _ => panic!("Wrong message type"),
        }
    }
}