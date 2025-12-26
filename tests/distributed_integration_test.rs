//! Integration tests for CrabCache distributed clustering
//!
//! These tests verify the complete distributed system functionality

use crabcache::cluster::distributed_pipeline::{PipelineCommand, RoutingStrategy};
use crabcache::cluster::{
    ClusterNode, ConsistentHashRing, DistributedPipelineManager, LoadBalancer,
    LoadBalancingStrategy, NodeCapabilities, NodeId,
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Create a test cluster node
fn create_test_node(id: u32) -> ClusterNode {
    let node_id = NodeId::generate();
    let addr = format!("127.0.0.1:{}", 8000 + id).parse().unwrap();
    let cluster_addr = format!("127.0.0.1:{}", 9000 + id).parse().unwrap();

    let mut capabilities = NodeCapabilities::default();
    capabilities.max_ops_per_sec = 500_000;
    capabilities.memory_capacity = 8 * 1024 * 1024 * 1024;
    capabilities.cpu_cores = 4;
    capabilities.simd_support = true;
    capabilities.zero_copy_support = true;
    capabilities.advanced_pipeline_support = true;
    capabilities.protocol_versions = vec!["1.0".to_string(), "2.0".to_string()];

    ClusterNode::new(node_id, addr, cluster_addr, capabilities)
}

#[tokio::test]
async fn test_consistent_hash_ring_distribution() {
    let mut hash_ring = ConsistentHashRing::new(256, 3);

    // Add nodes
    let node1 = create_test_node(1);
    let node2 = create_test_node(2);
    let node3 = create_test_node(3);

    hash_ring.add_node(node1);
    hash_ring.add_node(node2);
    hash_ring.add_node(node3);

    // Test key distribution
    let mut distribution = std::collections::HashMap::new();
    for i in 0..1000 {
        let key = format!("key_{}", i);
        if let Some(primary_node) = hash_ring.get_primary_node(key.as_bytes()) {
            *distribution.entry(primary_node).or_insert(0) += 1;
        }
    }

    // Check distribution is reasonably balanced (within 40% for small sample)
    let avg = 1000 / 3;
    for count in distribution.values() {
        assert!(
            *count > avg * 60 / 100,
            "Distribution too uneven: {}",
            count
        );
        assert!(
            *count < avg * 140 / 100,
            "Distribution too uneven: {}",
            count
        );
    }

    // Verify replication
    for i in 0..100 {
        let key = format!("test_key_{}", i);
        let replica_nodes = hash_ring.get_nodes_for_key(key.as_bytes());
        assert_eq!(replica_nodes.len(), 3, "Should have 3 replicas");
    }
}

#[tokio::test]
async fn test_load_balancer_strategies() {
    let nodes = vec![
        create_test_node(1),
        create_test_node(2),
        create_test_node(3),
    ];

    // Test Round Robin
    let round_robin_lb = LoadBalancer::new(LoadBalancingStrategy::RoundRobin);
    let mut selections = Vec::new();

    for _ in 0..9 {
        if let Ok(selected) = round_robin_lb.select_node(&nodes).await {
            selections.push(selected);
        }
    }

    // Should cycle through nodes
    assert_eq!(selections.len(), 9);

    // Test Weighted Round Robin
    let weighted_lb = LoadBalancer::new(LoadBalancingStrategy::WeightedRoundRobin);
    let mut weighted_selections = Vec::new();

    for _ in 0..6 {
        if let Ok(selected) = weighted_lb.select_node(&nodes).await {
            weighted_selections.push(selected);
        }
    }

    assert_eq!(weighted_selections.len(), 6);
}

#[tokio::test]
async fn test_distributed_pipeline_processing() {
    // Create hash ring with nodes
    let mut hash_ring = ConsistentHashRing::new(256, 3);
    let node1 = create_test_node(1);
    let node2 = create_test_node(2);
    let node3 = create_test_node(3);

    hash_ring.add_node(node1.clone());
    hash_ring.add_node(node2.clone());
    hash_ring.add_node(node3.clone());

    let hash_ring = Arc::new(RwLock::new(hash_ring));

    // Create distributed pipeline manager
    let pipeline_manager =
        DistributedPipelineManager::new(hash_ring, RoutingStrategy::ConsistentHash);

    // Test commands
    let commands = vec![
        PipelineCommand::Set {
            key: "test_key_1".to_string(),
            value: "value1".to_string(),
        },
        PipelineCommand::Set {
            key: "test_key_2".to_string(),
            value: "value2".to_string(),
        },
        PipelineCommand::Get {
            key: "test_key_1".to_string(),
        },
        PipelineCommand::Get {
            key: "test_key_2".to_string(),
        },
        PipelineCommand::Ping,
        PipelineCommand::Info,
    ];

    // Process commands
    let result = pipeline_manager
        .process_distributed_batch(commands.clone())
        .await;

    assert!(result.is_ok(), "Distributed processing should succeed");

    let response_batch = result.unwrap();
    assert_eq!(
        response_batch.responses.len(),
        commands.len(),
        "Should get response for each command"
    );

    // Check metrics
    let metrics = pipeline_manager.get_metrics().await;
    assert!(metrics.total_commands >= commands.len() as u64);
}

#[tokio::test]
async fn test_hash_ring_node_addition_removal() {
    let mut hash_ring = ConsistentHashRing::new(256, 3);

    // Start with 2 nodes
    let node1 = create_test_node(1);
    let node2 = create_test_node(2);

    hash_ring.add_node(node1.clone());
    hash_ring.add_node(node2.clone());

    // Test key distribution with 2 nodes
    let mut distribution_2_nodes = std::collections::HashMap::new();
    for i in 0..1000 {
        let key = format!("key_{}", i);
        if let Some(primary_node) = hash_ring.get_primary_node(key.as_bytes()) {
            *distribution_2_nodes.entry(primary_node).or_insert(0) += 1;
        }
    }

    assert_eq!(
        distribution_2_nodes.len(),
        2,
        "Should distribute across 2 nodes"
    );

    // Add third node
    let node3 = create_test_node(3);
    let migrations = hash_ring.add_node(node3.clone());

    // Should have some migrations when adding node
    assert!(
        !migrations.is_empty(),
        "Should have migrations when adding node"
    );

    // Test key distribution with 3 nodes
    let mut distribution_3_nodes = std::collections::HashMap::new();
    for i in 0..1000 {
        let key = format!("key_{}", i);
        if let Some(primary_node) = hash_ring.get_primary_node(key.as_bytes()) {
            *distribution_3_nodes.entry(primary_node).or_insert(0) += 1;
        }
    }

    assert_eq!(
        distribution_3_nodes.len(),
        3,
        "Should distribute across 3 nodes"
    );

    // Remove a node
    let removal_migrations = hash_ring.remove_node(node2.id);
    assert!(
        !removal_migrations.is_empty(),
        "Should have migrations when removing node"
    );

    // Verify only 2 nodes remain
    let mut distribution_after_removal = std::collections::HashMap::new();
    for i in 0..1000 {
        let key = format!("key_{}", i);
        if let Some(primary_node) = hash_ring.get_primary_node(key.as_bytes()) {
            *distribution_after_removal.entry(primary_node).or_insert(0) += 1;
        }
    }

    assert_eq!(
        distribution_after_removal.len(),
        2,
        "Should distribute across 2 nodes after removal"
    );
}

#[tokio::test]
async fn test_routing_strategies() {
    let mut hash_ring = ConsistentHashRing::new(256, 3);
    let node1 = create_test_node(1);
    let node2 = create_test_node(2);
    let node3 = create_test_node(3);

    hash_ring.add_node(node1);
    hash_ring.add_node(node2);
    hash_ring.add_node(node3);

    let hash_ring = Arc::new(RwLock::new(hash_ring));

    // Test different routing strategies
    let strategies = vec![
        RoutingStrategy::ConsistentHash,
        RoutingStrategy::LoadBased,
        RoutingStrategy::Hybrid {
            hash_weight: 0.7,
            load_weight: 0.3,
        },
        RoutingStrategy::LocalFirst,
    ];

    for strategy in strategies {
        let pipeline_manager = DistributedPipelineManager::new(hash_ring.clone(), strategy);

        let commands = vec![
            PipelineCommand::Get {
                key: "test_key".to_string(),
            },
            PipelineCommand::Set {
                key: "another_key".to_string(),
                value: "value".to_string(),
            },
        ];

        let result = pipeline_manager.process_distributed_batch(commands).await;
        assert!(result.is_ok(), "All routing strategies should work");
    }
}

#[tokio::test]
async fn test_performance_benchmarks() {
    let mut hash_ring = ConsistentHashRing::new(256, 3);
    let node1 = create_test_node(1);
    let node2 = create_test_node(2);
    let node3 = create_test_node(3);

    hash_ring.add_node(node1);
    hash_ring.add_node(node2);
    hash_ring.add_node(node3);

    let hash_ring = Arc::new(RwLock::new(hash_ring));
    let pipeline_manager =
        DistributedPipelineManager::new(hash_ring, RoutingStrategy::ConsistentHash);

    // Test different batch sizes
    let batch_sizes = vec![10, 50, 100, 200];

    for batch_size in batch_sizes {
        let mut commands = Vec::new();
        for i in 0..batch_size {
            commands.push(PipelineCommand::Get {
                key: format!("perf_key_{}", i),
            });
        }

        let start_time = std::time::Instant::now();
        let result = pipeline_manager
            .process_distributed_batch(commands.clone())
            .await;
        let duration = start_time.elapsed();

        assert!(result.is_ok(), "Performance test should succeed");

        let ops_per_sec = batch_size as f64 / duration.as_secs_f64();

        // Should achieve reasonable performance (>10k ops/sec for local processing)
        assert!(
            ops_per_sec > 10_000.0,
            "Performance too low: {:.0} ops/sec for batch size {}",
            ops_per_sec,
            batch_size
        );

        println!("Batch size {}: {:.0} ops/sec", batch_size, ops_per_sec);
    }
}

#[tokio::test]
async fn test_fault_tolerance_simulation() {
    let mut hash_ring = ConsistentHashRing::new(256, 3);
    let node1 = create_test_node(1);
    let node2 = create_test_node(2);
    let node3 = create_test_node(3);

    hash_ring.add_node(node1.clone());
    hash_ring.add_node(node2.clone());
    hash_ring.add_node(node3.clone());

    let hash_ring = Arc::new(RwLock::new(hash_ring));
    let pipeline_manager =
        DistributedPipelineManager::new(hash_ring.clone(), RoutingStrategy::ConsistentHash);

    // Add nodes to pipeline manager
    pipeline_manager.add_remote_node(node1.id, node1.address);
    pipeline_manager.add_remote_node(node2.id, node2.address);
    pipeline_manager.add_remote_node(node3.id, node3.address);

    let commands = vec![
        PipelineCommand::Set {
            key: "fault_test".to_string(),
            value: "data".to_string(),
        },
        PipelineCommand::Get {
            key: "fault_test".to_string(),
        },
    ];

    // Normal operation
    let result = pipeline_manager
        .process_distributed_batch(commands.clone())
        .await;
    assert!(result.is_ok(), "Normal operation should succeed");

    // Simulate node failure
    pipeline_manager.remove_remote_node(node1.id);

    // Should still work with remaining nodes
    let result_after_failure = pipeline_manager.process_distributed_batch(commands).await;
    assert!(
        result_after_failure.is_ok(),
        "Should handle node failure gracefully"
    );

    // Restore node
    pipeline_manager.add_remote_node(node1.id, node1.address);
}

#[tokio::test]
async fn test_cluster_metrics_collection() {
    let mut hash_ring = ConsistentHashRing::new(256, 3);
    let node1 = create_test_node(1);
    let node2 = create_test_node(2);

    hash_ring.add_node(node1);
    hash_ring.add_node(node2);

    let stats = hash_ring.get_stats();

    assert_eq!(stats.total_nodes, 2);
    assert_eq!(stats.virtual_nodes_per_node, 256);
    assert_eq!(stats.replication_factor, 3);
    assert!(stats.avg_load > 0.0);
    assert!(stats.max_load >= stats.min_load);

    let load_distribution = hash_ring.get_load_distribution();
    assert_eq!(load_distribution.len(), 2);

    // Verify load sums to approximately 1.0
    let total_load: f64 = load_distribution.values().sum();
    assert!(
        (total_load - 1.0).abs() < 0.1,
        "Total load should be close to 1.0"
    );
}
