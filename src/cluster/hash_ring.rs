//! Consistent hash ring implementation for auto-sharding
//!
//! This module provides consistent hashing functionality for distributing
//! data across cluster nodes with minimal reshuffling during node changes.

use crate::cluster::{ClusterError, ClusterNode, ClusterResult, NodeId};
use ahash::AHasher;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use tracing::{debug, info, warn};

/// Shard identifier
pub type ShardId = crate::cluster::node::ShardId;

/// Represents a data migration between nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMigration {
    pub migration_id: String,
    pub shard_id: ShardId,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub key_range: (u64, u64),
    pub status: MigrationStatus,
    pub progress: f64,
    pub estimated_keys: u64,
    pub transferred_keys: u64,
    #[serde(with = "instant_serde")]
    pub start_time: std::time::Instant,
}

mod instant_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
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
        Ok(Instant::now())
    }
}

/// Migration status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationStatus {
    Planned,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

/// Consistent hash ring for distributing keys across nodes
pub struct ConsistentHashRing {
    /// The ring mapping hash values to node IDs
    ring: BTreeMap<u64, NodeId>,
    /// Virtual nodes per physical node
    virtual_nodes: u32,
    /// Replication factor
    replication_factor: u32,
    /// Node information
    nodes: HashMap<NodeId, ClusterNode>,
    /// Total hash space (2^64)
    hash_space: u64,
}

impl ConsistentHashRing {
    /// Create a new consistent hash ring
    pub fn new(virtual_nodes: u32, replication_factor: u32) -> Self {
        Self {
            ring: BTreeMap::new(),
            virtual_nodes,
            replication_factor,
            nodes: HashMap::new(),
            hash_space: u64::MAX,
        }
    }

    /// Add a node to the ring
    pub fn add_node(&mut self, node: ClusterNode) -> Vec<ShardMigration> {
        let node_id = node.id;
        info!("Adding node {} to hash ring", node_id);

        // Store node information
        self.nodes.insert(node_id, node);

        // Add virtual nodes to ring
        for i in 0..self.virtual_nodes {
            let hash = self.hash_node_virtual(node_id, i);
            self.ring.insert(hash, node_id);
        }

        // Calculate migrations needed for rebalancing
        let migrations = self.calculate_migrations_for_new_node(node_id);

        info!(
            "Added node {} with {} virtual nodes, {} migrations needed",
            node_id,
            self.virtual_nodes,
            migrations.len()
        );

        migrations
    }

    /// Remove a node from the ring
    pub fn remove_node(&mut self, node_id: NodeId) -> Vec<ShardMigration> {
        info!("Removing node {} from hash ring", node_id);

        // Remove virtual nodes from ring
        let mut to_remove = Vec::new();
        for (&hash, &id) in &self.ring {
            if id == node_id {
                to_remove.push(hash);
            }
        }

        for hash in to_remove {
            self.ring.remove(&hash);
        }

        // Remove node information
        self.nodes.remove(&node_id);

        // Calculate migrations needed
        let migrations = self.calculate_migrations_for_removed_node(node_id);

        info!(
            "Removed node {}, {} migrations needed",
            node_id,
            migrations.len()
        );

        migrations
    }

    /// Get the primary and replica nodes for a key
    pub fn get_nodes_for_key(&self, key: &[u8]) -> Vec<NodeId> {
        if self.ring.is_empty() {
            return Vec::new();
        }

        let hash = self.hash_key(key);
        let mut nodes = Vec::new();
        let mut seen_nodes = std::collections::HashSet::new();

        // Find the first node at or after the hash
        let mut iter = self.ring.range(hash..);

        // If no node found after hash, wrap around to beginning
        if iter.clone().next().is_none() {
            iter = self.ring.range(..);
        }

        // Collect unique nodes up to replication factor
        for (_, &node_id) in iter {
            if seen_nodes.insert(node_id) {
                nodes.push(node_id);
                if nodes.len() >= self.replication_factor as usize {
                    break;
                }
            }
        }

        // If we still need more nodes, wrap around from the beginning
        if nodes.len() < self.replication_factor as usize {
            for (_, &node_id) in &self.ring {
                if seen_nodes.insert(node_id) {
                    nodes.push(node_id);
                    if nodes.len() >= self.replication_factor as usize {
                        break;
                    }
                }
            }
        }

        nodes
    }

    /// Get the primary node for a key
    pub fn get_primary_node(&self, key: &[u8]) -> Option<NodeId> {
        self.get_nodes_for_key(key).first().copied()
    }

    /// Get all replica nodes for a key (excluding primary)
    pub fn get_replica_nodes(&self, key: &[u8]) -> Vec<NodeId> {
        let mut nodes = self.get_nodes_for_key(key);
        if !nodes.is_empty() {
            nodes.remove(0); // Remove primary
        }
        nodes
    }

    /// Hash a key to a position on the ring
    fn hash_key(&self, key: &[u8]) -> u64 {
        let mut hasher = AHasher::default();
        key.hash(&mut hasher);
        hasher.finish()
    }

    /// Hash a virtual node to a position on the ring
    fn hash_node_virtual(&self, node_id: NodeId, virtual_index: u32) -> u64 {
        let mut hasher = AHasher::default();
        node_id.hash(&mut hasher);
        virtual_index.hash(&mut hasher);
        hasher.finish()
    }

    /// Calculate migrations needed when adding a new node
    fn calculate_migrations_for_new_node(&self, new_node_id: NodeId) -> Vec<ShardMigration> {
        let mut migrations = Vec::new();

        // For each virtual node of the new node, check what keys it should take over
        for i in 0..self.virtual_nodes {
            let hash = self.hash_node_virtual(new_node_id, i);

            // Find the next node in the ring (clockwise)
            let next_node = self
                .ring
                .range((hash + 1)..)
                .next()
                .or_else(|| self.ring.iter().next())
                .map(|(_, &node_id)| node_id);

            if let Some(from_node) = next_node {
                if from_node != new_node_id {
                    // Calculate the key range this virtual node should handle
                    let start_hash = self.get_previous_hash(hash);
                    let end_hash = hash;

                    let migration = ShardMigration {
                        migration_id: format!(
                            "migration_{}_{}_to_{}",
                            start_hash, from_node, new_node_id
                        ),
                        shard_id: ShardId::new((hash % 1024) as u32), // Simple shard mapping
                        from_node,
                        to_node: new_node_id,
                        key_range: (start_hash, end_hash),
                        status: MigrationStatus::Planned,
                        progress: 0.0,
                        estimated_keys: 0, // Will be calculated during migration
                        transferred_keys: 0,
                        start_time: std::time::Instant::now(),
                    };

                    migrations.push(migration);
                }
            }
        }

        migrations
    }

    /// Calculate migrations needed when removing a node
    fn calculate_migrations_for_removed_node(
        &self,
        removed_node_id: NodeId,
    ) -> Vec<ShardMigration> {
        let mut migrations = Vec::new();

        // Find all virtual nodes that belonged to the removed node
        let removed_hashes: Vec<u64> = self
            .ring
            .iter()
            .filter_map(|(&hash, &node_id)| {
                if node_id == removed_node_id {
                    Some(hash)
                } else {
                    None
                }
            })
            .collect();

        for hash in removed_hashes {
            // Find the next available node to take over this range
            let next_node = self
                .ring
                .range((hash + 1)..)
                .find(|(_, &node_id)| node_id != removed_node_id)
                .or_else(|| {
                    self.ring
                        .iter()
                        .find(|(_, &node_id)| node_id != removed_node_id)
                })
                .map(|(_, &node_id)| node_id);

            if let Some(to_node) = next_node {
                let start_hash = self.get_previous_hash(hash);
                let end_hash = hash;

                let migration = ShardMigration {
                    migration_id: format!(
                        "migration_{}_{}_to_{}",
                        start_hash, removed_node_id, to_node
                    ),
                    shard_id: ShardId::new((hash % 1024) as u32),
                    from_node: removed_node_id,
                    to_node,
                    key_range: (start_hash, end_hash),
                    status: MigrationStatus::Planned,
                    progress: 0.0,
                    estimated_keys: 0,
                    transferred_keys: 0,
                    start_time: std::time::Instant::now(),
                };

                migrations.push(migration);
            }
        }

        migrations
    }

    /// Get the previous hash value in the ring
    fn get_previous_hash(&self, hash: u64) -> u64 {
        self.ring
            .range(..hash)
            .next_back()
            .map(|(&prev_hash, _)| prev_hash)
            .unwrap_or(0)
    }

    /// Get current load distribution across nodes
    pub fn get_load_distribution(&self) -> HashMap<NodeId, f64> {
        let mut distribution = HashMap::new();

        if self.ring.is_empty() {
            return distribution;
        }

        // Count virtual nodes per physical node
        let mut virtual_node_counts = HashMap::new();
        for &node_id in self.ring.values() {
            *virtual_node_counts.entry(node_id).or_insert(0) += 1;
        }

        // Calculate load as percentage of total virtual nodes
        let total_virtual_nodes = self.ring.len() as f64;
        for (node_id, count) in virtual_node_counts {
            let load = count as f64 / total_virtual_nodes;
            distribution.insert(node_id, load);
        }

        distribution
    }

    /// Check if the ring is balanced within the given threshold
    pub fn is_balanced(&self, threshold: f64) -> bool {
        let distribution = self.get_load_distribution();

        if distribution.is_empty() {
            return true;
        }

        let expected_load = 1.0 / distribution.len() as f64;

        for load in distribution.values() {
            let deviation = (load - expected_load).abs() / expected_load;
            if deviation > threshold {
                return false;
            }
        }

        true
    }

    /// Get ring statistics
    pub fn get_stats(&self) -> HashRingStats {
        let distribution = self.get_load_distribution();

        let loads: Vec<f64> = distribution.values().copied().collect();
        let avg_load = if loads.is_empty() {
            0.0
        } else {
            loads.iter().sum::<f64>() / loads.len() as f64
        };
        let max_load = loads.iter().fold(0.0f64, |a, &b| a.max(b));
        let min_load = loads.iter().fold(1.0f64, |a, &b| a.min(b));
        let load_variance = if loads.is_empty() {
            0.0
        } else {
            loads.iter().map(|&x| (x - avg_load).powi(2)).sum::<f64>() / loads.len() as f64
        };

        HashRingStats {
            total_nodes: self.nodes.len() as u32,
            virtual_nodes_per_node: self.virtual_nodes,
            replication_factor: self.replication_factor,
            total_virtual_nodes: self.ring.len() as u32,
            avg_load,
            max_load,
            min_load,
            load_variance,
            is_balanced: self.is_balanced(0.1), // 10% threshold
        }
    }

    /// Get all nodes in the ring
    pub fn get_nodes(&self) -> Vec<&ClusterNode> {
        self.nodes.values().collect()
    }

    /// Get node by ID
    pub fn get_node(&self, node_id: NodeId) -> Option<&ClusterNode> {
        self.nodes.get(&node_id)
    }

    /// Update node information
    pub fn update_node(&mut self, node: ClusterNode) {
        self.nodes.insert(node.id, node);
    }
}

/// Hash ring statistics
#[derive(Debug, Clone)]
pub struct HashRingStats {
    pub total_nodes: u32,
    pub virtual_nodes_per_node: u32,
    pub replication_factor: u32,
    pub total_virtual_nodes: u32,
    pub avg_load: f64,
    pub max_load: f64,
    pub min_load: f64,
    pub load_variance: f64,
    pub is_balanced: bool,
}

/// Auto-sharding manager that handles shard distribution and rebalancing
pub struct AutoShardingManager {
    hash_ring: ConsistentHashRing,
    migration_queue: VecDeque<ShardMigration>,
    rebalance_threshold: f64,
    max_concurrent_migrations: u32,
    active_migrations: HashMap<String, ShardMigration>,
}

impl AutoShardingManager {
    /// Create new auto-sharding manager
    pub fn new(
        virtual_nodes: u32,
        replication_factor: u32,
        rebalance_threshold: f64,
        max_concurrent_migrations: u32,
    ) -> Self {
        Self {
            hash_ring: ConsistentHashRing::new(virtual_nodes, replication_factor),
            migration_queue: VecDeque::new(),
            rebalance_threshold,
            max_concurrent_migrations,
            active_migrations: HashMap::new(),
        }
    }

    /// Add node and trigger rebalancing if needed
    pub fn add_node(&mut self, node: ClusterNode) -> ClusterResult<Vec<ShardMigration>> {
        let migrations = self.hash_ring.add_node(node);

        // Add migrations to queue
        for migration in &migrations {
            self.migration_queue.push_back(migration.clone());
        }

        Ok(migrations)
    }

    /// Remove node and trigger rebalancing
    pub fn remove_node(&mut self, node_id: NodeId) -> ClusterResult<Vec<ShardMigration>> {
        let migrations = self.hash_ring.remove_node(node_id);

        // Add migrations to queue
        for migration in &migrations {
            self.migration_queue.push_back(migration.clone());
        }

        Ok(migrations)
    }

    /// Check if cluster needs rebalancing
    pub fn needs_rebalancing(&self) -> bool {
        !self.hash_ring.is_balanced(self.rebalance_threshold)
    }

    /// Get next migration to execute
    pub fn get_next_migration(&mut self) -> Option<ShardMigration> {
        if self.active_migrations.len() >= self.max_concurrent_migrations as usize {
            return None;
        }

        self.migration_queue.pop_front()
    }

    /// Start migration execution
    pub fn start_migration(&mut self, migration: ShardMigration) -> ClusterResult<()> {
        if self.active_migrations.len() >= self.max_concurrent_migrations as usize {
            return Err(ClusterError::ConfigError {
                message: "Too many concurrent migrations".to_string(),
            });
        }

        let migration_id = migration.migration_id.clone();
        self.active_migrations.insert(migration_id, migration);

        Ok(())
    }

    /// Complete migration
    pub fn complete_migration(&mut self, migration_id: &str) -> ClusterResult<()> {
        if let Some(mut migration) = self.active_migrations.remove(migration_id) {
            migration.status = MigrationStatus::Completed;
            migration.progress = 1.0;
            info!("Migration {} completed", migration_id);
            Ok(())
        } else {
            Err(ClusterError::MigrationFailed {
                migration_id: migration_id.to_string(),
            })
        }
    }

    /// Fail migration
    pub fn fail_migration(&mut self, migration_id: &str, reason: &str) -> ClusterResult<()> {
        if let Some(mut migration) = self.active_migrations.remove(migration_id) {
            migration.status = MigrationStatus::Failed;
            warn!("Migration {} failed: {}", migration_id, reason);

            // Re-queue for retry (simplified)
            self.migration_queue.push_back(migration);
            Ok(())
        } else {
            Err(ClusterError::MigrationFailed {
                migration_id: migration_id.to_string(),
            })
        }
    }

    /// Update migration progress
    pub fn update_migration_progress(
        &mut self,
        migration_id: &str,
        progress: f64,
    ) -> ClusterResult<()> {
        if let Some(migration) = self.active_migrations.get_mut(migration_id) {
            migration.progress = progress.clamp(0.0, 1.0);
            debug!(
                "Migration {} progress: {:.1}%",
                migration_id,
                progress * 100.0
            );
            Ok(())
        } else {
            Err(ClusterError::MigrationFailed {
                migration_id: migration_id.to_string(),
            })
        }
    }

    /// Get nodes for key
    pub fn get_nodes_for_key(&self, key: &[u8]) -> Vec<NodeId> {
        self.hash_ring.get_nodes_for_key(key)
    }

    /// Get primary node for key
    pub fn get_primary_node(&self, key: &[u8]) -> Option<NodeId> {
        self.hash_ring.get_primary_node(key)
    }

    /// Get hash ring statistics
    pub fn get_stats(&self) -> HashRingStats {
        self.hash_ring.get_stats()
    }

    /// Get active migrations
    pub fn get_active_migrations(&self) -> Vec<&ShardMigration> {
        self.active_migrations.values().collect()
    }

    /// Get queued migrations count
    pub fn get_queued_migrations_count(&self) -> usize {
        self.migration_queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::node::{NodeCapabilities, NodeId};

    fn create_test_node(id: u32) -> ClusterNode {
        let node_id = NodeId::generate();
        let addr = format!("127.0.0.1:{}", 8000 + id).parse().unwrap();
        let cluster_addr = format!("127.0.0.1:{}", 9000 + id).parse().unwrap();
        let capabilities = NodeCapabilities::default();

        crate::cluster::ClusterNode::new(node_id, addr, cluster_addr, capabilities)
    }

    #[test]
    fn test_consistent_hash_ring_creation() {
        let ring = ConsistentHashRing::new(256, 3);
        assert_eq!(ring.virtual_nodes, 256);
        assert_eq!(ring.replication_factor, 3);
        assert!(ring.ring.is_empty());
    }

    #[test]
    fn test_add_remove_nodes() {
        let mut ring = ConsistentHashRing::new(256, 3);

        let node1 = create_test_node(1);
        let node2 = create_test_node(2);
        let node3 = create_test_node(3);

        let node1_id = node1.id;
        let node2_id = node2.id;
        let node3_id = node3.id;

        // Add nodes
        ring.add_node(node1);
        ring.add_node(node2);
        ring.add_node(node3);

        assert_eq!(ring.ring.len(), 256 * 3); // 256 virtual nodes per physical node
        assert_eq!(ring.nodes.len(), 3);

        // Remove node
        ring.remove_node(node2_id);
        assert_eq!(ring.ring.len(), 256 * 2);
        assert_eq!(ring.nodes.len(), 2);

        // Verify remaining nodes
        assert!(ring.nodes.contains_key(&node1_id));
        assert!(!ring.nodes.contains_key(&node2_id));
        assert!(ring.nodes.contains_key(&node3_id));
    }

    #[test]
    fn test_key_distribution() {
        let mut ring = ConsistentHashRing::new(256, 3);

        let node1 = create_test_node(1);
        let node2 = create_test_node(2);
        let node3 = create_test_node(3);

        ring.add_node(node1);
        ring.add_node(node2);
        ring.add_node(node3);

        // Test key distribution
        let mut distribution = HashMap::new();
        for i in 0..10000 {
            let key = format!("key_{}", i);
            let nodes = ring.get_nodes_for_key(key.as_bytes());

            assert_eq!(nodes.len(), 3); // Should return 3 nodes (replication factor)

            let primary = nodes[0];
            *distribution.entry(primary).or_insert(0) += 1;
        }

        // Check distribution is reasonably balanced (within 30% of average)
        let avg = 10000 / 3;
        for count in distribution.values() {
            assert!(
                *count > avg * 70 / 100,
                "Count {} too low, avg {}",
                count,
                avg
            );
            assert!(
                *count < avg * 130 / 100,
                "Count {} too high, avg {}",
                count,
                avg
            );
        }
    }

    #[test]
    fn test_load_distribution() {
        let mut ring = ConsistentHashRing::new(256, 3);

        let node1 = create_test_node(1);
        let node2 = create_test_node(2);
        let node3 = create_test_node(3);

        ring.add_node(node1);
        ring.add_node(node2);
        ring.add_node(node3);

        let distribution = ring.get_load_distribution();
        assert_eq!(distribution.len(), 3);

        // Each node should have approximately 1/3 of the load
        for load in distribution.values() {
            assert!(*load > 0.25 && *load < 0.4, "Load {} not balanced", load);
        }

        // Total load should be 1.0
        let total_load: f64 = distribution.values().sum();
        assert!(
            (total_load - 1.0).abs() < 0.001,
            "Total load {} != 1.0",
            total_load
        );
    }

    #[test]
    fn test_balance_detection() {
        let mut ring = ConsistentHashRing::new(256, 3);

        // Empty ring is balanced
        assert!(ring.is_balanced(0.1));

        // Add nodes
        let node1 = create_test_node(1);
        let node2 = create_test_node(2);
        let node3 = create_test_node(3);

        ring.add_node(node1);
        ring.add_node(node2);
        ring.add_node(node3);

        // Should be reasonably balanced with 256 virtual nodes
        assert!(ring.is_balanced(0.2)); // 20% threshold
    }

    #[test]
    fn test_auto_sharding_manager() {
        let mut manager = AutoShardingManager::new(256, 3, 0.2, 3);

        let node1 = create_test_node(1);
        let node2 = create_test_node(2);

        // Add nodes
        let migrations1 = manager.add_node(node1).unwrap();
        let migrations2 = manager.add_node(node2).unwrap();

        // First node should have no migrations
        assert!(migrations1.is_empty());

        // Second node should trigger migrations
        assert!(!migrations2.is_empty());

        // Test migration queue
        assert!(manager.get_queued_migrations_count() > 0);

        // Get next migration
        let migration = manager.get_next_migration();
        assert!(migration.is_some());

        if let Some(migration) = migration {
            // Start migration
            let migration_id = migration.migration_id.clone();
            manager.start_migration(migration).unwrap();

            // Update progress
            manager
                .update_migration_progress(&migration_id, 0.5)
                .unwrap();

            // Complete migration
            manager.complete_migration(&migration_id).unwrap();
        }
    }

    #[test]
    fn test_migration_status() {
        let migration = ShardMigration {
            migration_id: "test_migration".to_string(),
            shard_id: ShardId::new(1),
            from_node: NodeId::generate(),
            to_node: NodeId::generate(),
            key_range: (0, 1000),
            status: MigrationStatus::Planned,
            progress: 0.0,
            estimated_keys: 1000,
            transferred_keys: 0,
            start_time: std::time::Instant::now(),
        };

        assert_eq!(migration.status, MigrationStatus::Planned);
        assert_eq!(migration.progress, 0.0);
        assert_eq!(migration.estimated_keys, 1000);
    }

    #[test]
    fn test_hash_ring_stats() {
        let mut ring = ConsistentHashRing::new(256, 3);

        let node1 = create_test_node(1);
        let node2 = create_test_node(2);

        ring.add_node(node1);
        ring.add_node(node2);

        let stats = ring.get_stats();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.virtual_nodes_per_node, 256);
        assert_eq!(stats.replication_factor, 3);
        assert_eq!(stats.total_virtual_nodes, 512);
        assert!(stats.avg_load > 0.0);
        assert!(stats.max_load <= 1.0);
        assert!(stats.min_load >= 0.0);
    }
}
