//! Example demonstrating TinyLFU eviction system usage

use crabcache::eviction::{EvictionConfig, EvictionPolicy, TinyLFU};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("CrabCache TinyLFU Eviction Example");
    println!("==================================");

    // Create TinyLFU configuration using Default and customizing specific fields
    let mut config = EvictionConfig::default();
    config.max_capacity = 5;   // Small capacity for demonstration
    config.window_ratio = 0.2; // 20% for window LRU (1 item)
    config.sketch_width = 64;  // Small sketch for demo
    config.sketch_depth = 4;
    config.memory_high_watermark = 0.8;
    config.memory_low_watermark = 0.6;
    config.reset_interval_secs = 60;
    config.enabled = true;

    // Create TinyLFU cache
    let mut cache = TinyLFU::new(config)?;

    println!("Created TinyLFU cache with capacity: {}", cache.capacity());
    println!("Window size: 1, Main size: 4");
    println!();

    // Demonstrate basic operations
    println!("1. Basic PUT/GET operations:");
    cache.put("key1".to_string(), b"value1".to_vec());
    cache.put("key2".to_string(), b"value2".to_vec());
    cache.put("key3".to_string(), b"value3".to_vec());

    println!("   PUT key1, key2, key3");
    println!("   Cache size: {}", cache.len());

    if let Some(value) = cache.get("key1") {
        println!("   GET key1: {}", String::from_utf8_lossy(&value));
    }

    println!();

    // Demonstrate frequency-based eviction
    println!("2. Frequency-based eviction:");

    // Access key1 multiple times to increase its frequency
    for i in 0..10 {
        cache.get("key1");
        if i % 3 == 0 {
            println!("   Accessed key1 {} times", i + 1);
        }
    }

    // Access key2 a few times
    for _ in 0..3 {
        cache.get("key2");
    }
    println!("   Accessed key2 3 times");

    // Fill cache to trigger eviction
    cache.put("key4".to_string(), b"value4".to_vec());
    cache.put("key5".to_string(), b"value5".to_vec());
    cache.put("key6".to_string(), b"value6".to_vec()); // Should trigger eviction

    println!("   Added key4, key5, key6 (triggers eviction)");
    println!("   Cache size: {}", cache.len());

    // Check which keys survived
    let keys = ["key1", "key2", "key3", "key4", "key5", "key6"];
    println!("   Keys still in cache:");
    for key in &keys {
        if cache.get(key).is_some() {
            println!("     ✓ {}", key);
        } else {
            println!("     ✗ {} (evicted)", key);
        }
    }

    println!();

    // Show metrics
    println!("3. Cache metrics:");
    let metrics = cache.metrics().snapshot();
    println!("   Total requests: {}", metrics.total_requests);
    println!("   Cache hits: {}", metrics.cache_hits);
    println!("   Cache misses: {}", metrics.cache_misses);
    println!("   Hit ratio: {:.2}%", metrics.hit_ratio * 100.0);
    println!("   Evictions: {}", metrics.evictions);
    println!("   Window promotions: {}", metrics.window_promotions);
    println!("   Admissions accepted: {}", metrics.admissions_accepted);
    println!("   Admissions rejected: {}", metrics.admissions_rejected);
    println!(
        "   Admission ratio: {:.2}%",
        metrics.admission_ratio * 100.0
    );

    println!();

    // Demonstrate batch operations
    println!("4. Batch operations:");
    cache.reset_metrics();

    let batch_items = vec![
        ("batch1".to_string(), b"bvalue1".to_vec()),
        ("batch2".to_string(), b"bvalue2".to_vec()),
        ("batch3".to_string(), b"bvalue3".to_vec()),
    ];

    let evicted = cache.put_batch(batch_items);
    println!("   Added 3 items in batch");
    println!("   Items evicted: {}", evicted.len());

    for (key, _) in evicted {
        println!("     Evicted: {}", key);
    }

    println!("   Final cache size: {}", cache.len());

    // Final metrics
    let final_metrics = cache.metrics().snapshot();
    println!(
        "   Final hit ratio: {:.2}%",
        final_metrics.hit_ratio * 100.0
    );

    println!();
    println!("TinyLFU demonstration complete!");

    Ok(())
}
