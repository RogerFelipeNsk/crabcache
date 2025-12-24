//! Store and binary layout integration tests

use bytes::Bytes;
use crabcache::store::{Item, ShardStore};

#[tokio::test]
async fn test_item_binary_layout() {
    let key = Bytes::from("test_key");
    let value = Bytes::from("test_value");
    let item = Item::new(key.clone(), value.clone(), None);

    // Test serialization
    let binary = item.to_binary();
    assert!(!binary.is_empty());

    // Test deserialization
    let deserialized = Item::from_binary(binary).unwrap();
    assert_eq!(deserialized.key, key);
    assert_eq!(deserialized.value, value);
    assert_eq!(deserialized.expires_at, None);
    assert_eq!(deserialized.flags, 0);
}

#[tokio::test]
async fn test_item_with_ttl() {
    let key = Bytes::from("ttl_key");
    let value = Bytes::from("ttl_value");
    let item = Item::with_ttl(key.clone(), value.clone(), 3600);

    // Should not be expired immediately
    assert!(!item.is_expired());
    assert!(item.expires_at.is_some());

    // Test serialization roundtrip
    let binary = item.to_binary();
    let deserialized = Item::from_binary(binary).unwrap();
    assert_eq!(deserialized.key, key);
    assert_eq!(deserialized.value, value);
    assert!(deserialized.expires_at.is_some());
}

#[tokio::test]
async fn test_item_expiration() {
    let key = Bytes::from("expire_key");
    let value = Bytes::from("expire_value");

    // Create item that expires in 0 seconds (immediately)
    let item = Item::with_ttl(key, value, 0);

    // Should be expired
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    assert!(item.is_expired());
}

#[tokio::test]
async fn test_shard_store_basic_operations() {
    let mut store = ShardStore::new(1024 * 1024); // 1MB

    let key = Bytes::from("store_key");
    let value = Bytes::from("store_value");

    // Test PUT
    assert!(store.put(key.clone(), value.clone(), None));
    assert_eq!(store.len(), 1);

    // Test GET
    let retrieved = store.get(&key).unwrap();
    assert_eq!(retrieved, value);

    // Test DEL
    assert!(store.del(&key));
    assert_eq!(store.len(), 0);

    // Test GET after DEL
    assert!(store.get(&key).is_none());
}

#[tokio::test]
async fn test_shard_store_ttl() {
    let mut store = ShardStore::new(1024 * 1024);

    let key = Bytes::from("ttl_store_key");
    let value = Bytes::from("ttl_store_value");

    // PUT with TTL
    assert!(store.put(key.clone(), value.clone(), Some(1))); // 1 second TTL
    assert_eq!(store.len(), 1);

    // Should be available immediately
    assert!(store.get(&key).is_some());

    // Wait for expiration
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Should be expired and removed
    assert!(store.get(&key).is_none());
    assert_eq!(store.len(), 0);
}

#[tokio::test]
async fn test_shard_store_expire_command() {
    let mut store = ShardStore::new(1024 * 1024);

    let key = Bytes::from("expire_cmd_key");
    let value = Bytes::from("expire_cmd_value");

    // PUT without TTL
    assert!(store.put(key.clone(), value.clone(), None));

    // Set TTL using EXPIRE
    assert!(store.expire(&key, 1)); // 1 second

    // Should be available immediately
    assert!(store.get(&key).is_some());

    // Wait for expiration
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Should be expired
    assert!(store.get(&key).is_none());
}

#[tokio::test]
async fn test_shard_store_memory_limit() {
    let mut store = ShardStore::new(50); // Very small limit

    let key1 = Bytes::from("k1");
    let value1 = Bytes::from("small_value");

    // First PUT should succeed
    assert!(store.put(key1.clone(), value1.clone(), None));

    let key2 = Bytes::from("k2");
    let value2 =
        Bytes::from("this_is_a_very_long_value_that_should_definitely_exceed_the_memory_limit");

    // Second PUT should fail due to memory limit
    assert!(!store.put(key2, value2, None));

    // First key should still be there
    assert!(store.get(&key1).is_some());
}

#[tokio::test]
async fn test_shard_store_memory_stats() {
    let mut store = ShardStore::new(1024 * 1024);

    let (memory_used, max_memory, key_count) = store.memory_stats();
    assert_eq!(memory_used, 0);
    assert_eq!(max_memory, 1024 * 1024);
    assert_eq!(key_count, 0);

    // Add some data
    let key = Bytes::from("stats_key");
    let value = Bytes::from("stats_value");
    assert!(store.put(key, value, None));

    let (memory_used, max_memory, key_count) = store.memory_stats();
    assert!(memory_used > 0);
    assert_eq!(max_memory, 1024 * 1024);
    assert_eq!(key_count, 1);
}

#[tokio::test]
async fn test_shard_store_cleanup_expired() {
    let mut store = ShardStore::new(1024 * 1024);

    // Add items with different TTLs
    let key1 = Bytes::from("key1");
    let value1 = Bytes::from("value1");
    assert!(store.put(key1, value1, Some(1))); // 1 second

    let key2 = Bytes::from("key2");
    let value2 = Bytes::from("value2");
    assert!(store.put(key2, value2, Some(3600))); // 1 hour

    let key3 = Bytes::from("key3");
    let value3 = Bytes::from("value3");
    assert!(store.put(key3, value3, None)); // No TTL

    assert_eq!(store.len(), 3);

    // Wait for first item to expire
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Cleanup expired items
    let removed = store.cleanup_expired();
    assert_eq!(removed, 1);
    assert_eq!(store.len(), 2);
}

#[tokio::test]
async fn test_binary_layout_various_sizes() {
    let test_cases =
        vec![
        ("", ""),
        ("a", "b"),
        ("short", "value"),
        ("medium_length_key", "medium_length_value_here"),
        ("very_long_key_that_exceeds_normal_expectations", 
         "very_long_value_that_also_exceeds_normal_expectations_and_should_test_varint_encoding"),
    ];

    for (key_str, value_str) in test_cases {
        let key = Bytes::from(key_str);
        let value = Bytes::from(value_str);
        let item = Item::new(key.clone(), value.clone(), Some(1234567890));

        let binary = item.to_binary();
        let deserialized = Item::from_binary(binary).unwrap();

        assert_eq!(deserialized.key, key);
        assert_eq!(deserialized.value, value);
        assert_eq!(deserialized.expires_at, Some(1234567890));
    }
}
