//! Sharding system integration tests

use bytes::Bytes;
use crabcache::protocol::commands::{Command, Response};
use crabcache::router::ShardRouter;
use crabcache::utils::hash::hash_key;

const DEFAULT_MAX_MEMORY: usize = 1024 * 1024; // 1MB per shard

#[tokio::test]
async fn test_shard_routing() {
    let router = ShardRouter::new(4, DEFAULT_MAX_MEMORY);

    let test_keys = vec![
        b"key1".as_slice(),
        b"key2".as_slice(),
        b"key3".as_slice(),
        b"key4".as_slice(),
        b"key5".as_slice(),
    ];

    // Test that keys are consistently routed to the same shard
    for key in &test_keys {
        let shard1 = router.route_key(key);
        let shard2 = router.route_key(key);
        assert_eq!(shard1, shard2, "Key should always route to the same shard");
        assert!(shard1 < 4, "Shard index should be within bounds");
    }
}

#[tokio::test]
async fn test_put_and_get() {
    let router = ShardRouter::new(2, DEFAULT_MAX_MEMORY);

    // Test PUT command
    let put_command = Command::Put {
        key: Bytes::from("test_key"),
        value: Bytes::from("test_value"),
        ttl: None,
    };

    let response = router.process_command(put_command).await;
    assert_eq!(response, Response::Ok);

    // Test GET command
    let get_command = Command::Get {
        key: Bytes::from("test_key"),
    };

    let response = router.process_command(get_command).await;
    match response {
        Response::Value(value) => {
            assert_eq!(value, Bytes::from("test_value"));
        }
        _ => panic!("Expected Value response, got {:?}", response),
    }
}

#[tokio::test]
async fn test_put_get_del() {
    let router = ShardRouter::new(3, DEFAULT_MAX_MEMORY);

    let key = Bytes::from("delete_test");
    let value = Bytes::from("delete_value");

    // PUT
    let put_cmd = Command::Put {
        key: key.clone(),
        value: value.clone(),
        ttl: None,
    };
    let response = router.process_command(put_cmd).await;
    assert_eq!(response, Response::Ok);

    // GET (should exist)
    let get_cmd = Command::Get { key: key.clone() };
    let response = router.process_command(get_cmd).await;
    match response {
        Response::Value(v) => assert_eq!(v, value),
        _ => panic!("Expected value, got {:?}", response),
    }

    // DEL
    let del_cmd = Command::Del { key: key.clone() };
    let response = router.process_command(del_cmd).await;
    assert_eq!(response, Response::Ok);

    // GET (should not exist)
    let get_cmd = Command::Get { key: key.clone() };
    let response = router.process_command(get_cmd).await;
    assert_eq!(response, Response::Null);
}

#[tokio::test]
async fn test_multiple_keys_different_shards() {
    let router = ShardRouter::new(4, DEFAULT_MAX_MEMORY);

    let keys_values = vec![
        ("key1", "value1"),
        ("key2", "value2"),
        ("key3", "value3"),
        ("key4", "value4"),
        ("key5", "value5"),
    ];

    // PUT all keys
    for (key, value) in &keys_values {
        let put_cmd = Command::Put {
            key: Bytes::from(*key),
            value: Bytes::from(*value),
            ttl: None,
        };
        let response = router.process_command(put_cmd).await;
        assert_eq!(response, Response::Ok);
    }

    // GET all keys
    for (key, expected_value) in &keys_values {
        let get_cmd = Command::Get {
            key: Bytes::from(*key),
        };
        let response = router.process_command(get_cmd).await;
        match response {
            Response::Value(value) => {
                assert_eq!(value, Bytes::from(*expected_value));
            }
            _ => panic!("Expected value for key {}, got {:?}", key, response),
        }
    }
}

#[tokio::test]
async fn test_stats_command() {
    let router = ShardRouter::new(2, DEFAULT_MAX_MEMORY);

    // Add some data
    let put_cmd = Command::Put {
        key: Bytes::from("stats_key"),
        value: Bytes::from("stats_value"),
        ttl: None,
    };
    router.process_command(put_cmd).await;

    // Get stats
    let stats_cmd = Command::Stats;
    let response = router.process_command(stats_cmd).await;

    match response {
        Response::Stats(stats) => {
            assert!(stats.contains("shard_"));
            assert!(stats.contains("total:"));
            assert!(stats.contains("keys"));
            assert!(stats.contains("memory"));
        }
        _ => panic!("Expected Stats response, got {:?}", response),
    }
}

#[tokio::test]
async fn test_hash_distribution() {
    // Test that hash function distributes keys reasonably
    let num_shards = 4;
    let mut shard_counts = vec![0; num_shards];

    // Generate many keys and count distribution
    for i in 0..1000 {
        let key = format!("key_{}", i);
        let hash = hash_key(key.as_bytes());
        let shard = (hash as usize) % num_shards;
        shard_counts[shard] += 1;
    }

    // Each shard should have some keys (not perfect distribution, but reasonable)
    for count in shard_counts {
        assert!(
            count > 200,
            "Shard should have reasonable number of keys: {}",
            count
        );
        assert!(
            count < 300,
            "Shard should not have too many keys: {}",
            count
        );
    }
}

#[tokio::test]
async fn test_ping_command() {
    let router = ShardRouter::new(1, DEFAULT_MAX_MEMORY);

    let ping_cmd = Command::Ping;
    let response = router.process_command(ping_cmd).await;
    assert_eq!(response, Response::Pong);
}

#[tokio::test]
async fn test_ttl_functionality() {
    let router = ShardRouter::new(2, DEFAULT_MAX_MEMORY);

    // PUT with TTL
    let put_cmd = Command::Put {
        key: Bytes::from("ttl_key"),
        value: Bytes::from("ttl_value"),
        ttl: Some(1), // 1 second
    };
    let response = router.process_command(put_cmd).await;
    assert_eq!(response, Response::Ok);

    // Should be available immediately
    let get_cmd = Command::Get {
        key: Bytes::from("ttl_key"),
    };
    let response = router.process_command(get_cmd).await;
    assert!(matches!(response, Response::Value(_)));

    // Wait for expiration
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Should be expired
    let get_cmd = Command::Get {
        key: Bytes::from("ttl_key"),
    };
    let response = router.process_command(get_cmd).await;
    assert_eq!(response, Response::Null);
}

#[tokio::test]
async fn test_expire_command() {
    let router = ShardRouter::new(2, DEFAULT_MAX_MEMORY);

    // PUT without TTL
    let put_cmd = Command::Put {
        key: Bytes::from("expire_key"),
        value: Bytes::from("expire_value"),
        ttl: None,
    };
    router.process_command(put_cmd).await;

    // Set TTL using EXPIRE
    let expire_cmd = Command::Expire {
        key: Bytes::from("expire_key"),
        ttl: 1, // 1 second
    };
    let response = router.process_command(expire_cmd).await;
    assert_eq!(response, Response::Ok);

    // Should be available immediately
    let get_cmd = Command::Get {
        key: Bytes::from("expire_key"),
    };
    let response = router.process_command(get_cmd).await;
    assert!(matches!(response, Response::Value(_)));

    // Wait for expiration
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Should be expired
    let get_cmd = Command::Get {
        key: Bytes::from("expire_key"),
    };
    let response = router.process_command(get_cmd).await;
    assert_eq!(response, Response::Null);
}
