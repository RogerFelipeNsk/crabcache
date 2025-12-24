//! WAL persistence example

use bytes::Bytes;
use crabcache::eviction::EvictionConfig;
use crabcache::protocol::commands::{Command, Response};
use crabcache::shard::WALShardManager;
use crabcache::wal::{SyncPolicy, WALConfig};
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 CrabCache WAL Persistence Example");
    println!("=====================================");

    // Create temporary directory for WAL
    let temp_dir = TempDir::new()?;
    let wal_dir = temp_dir.path().to_path_buf();

    println!("📁 WAL Directory: {:?}", wal_dir);

    // Configure WAL
    let wal_config = WALConfig {
        wal_dir: wal_dir.clone(),
        max_segment_size: 1024 * 1024, // 1MB segments
        buffer_size: 4096,             // 4KB buffer
        flush_interval_ms: 1000,       // 1 second flush
        sync_policy: SyncPolicy::Sync, // Sync for durability
    };

    // Configure eviction
    let eviction_config = EvictionConfig {
        max_capacity: 1000,
        ..Default::default()
    };

    println!("⚙️  Creating WAL-enabled shard manager...");

    // Create WAL-enabled manager
    let (manager, recovery_stats) = WALShardManager::new_with_recovery(
        2,           // 2 shards
        1024 * 1024, // 1MB per shard
        eviction_config,
        Some(wal_config),
    )
    .await?;

    if let Some(stats) = recovery_stats {
        println!("📊 Recovery Stats: {:?}", stats);
    }

    println!("✅ WAL manager created successfully!");

    // Test basic operations
    println!("\n🔧 Testing basic operations...");

    // PUT operations
    let put_cmd1 = Command::Put {
        key: Bytes::from("user:1001"),
        value: Bytes::from(r#"{"name": "Alice", "age": 30}"#),
        ttl: None,
    };

    let put_cmd2 = Command::Put {
        key: Bytes::from("user:1002"),
        value: Bytes::from(r#"{"name": "Bob", "age": 25}"#),
        ttl: Some(3600), // 1 hour TTL
    };

    let put_cmd3 = Command::Put {
        key: Bytes::from("counter:visits"),
        value: Bytes::from("42"),
        ttl: None,
    };

    println!("📝 Storing user:1001...");
    let response = manager.process_command(put_cmd1).await;
    println!("   Response: {:?}", response);

    println!("📝 Storing user:1002 with TTL...");
    let response = manager.process_command(put_cmd2).await;
    println!("   Response: {:?}", response);

    println!("📝 Storing counter:visits...");
    let response = manager.process_command(put_cmd3).await;
    println!("   Response: {:?}", response);

    // GET operations
    println!("\n🔍 Testing GET operations...");

    let get_cmd1 = Command::Get {
        key: Bytes::from("user:1001"),
    };

    let get_cmd2 = Command::Get {
        key: Bytes::from("user:1002"),
    };

    let get_cmd3 = Command::Get {
        key: Bytes::from("nonexistent"),
    };

    println!("🔍 Getting user:1001...");
    let response = manager.process_command(get_cmd1).await;
    match response {
        Response::Value(value) => {
            println!("   Found: {}", String::from_utf8_lossy(&value));
        }
        other => println!("   Response: {:?}", other),
    }

    println!("🔍 Getting user:1002...");
    let response = manager.process_command(get_cmd2).await;
    match response {
        Response::Value(value) => {
            println!("   Found: {}", String::from_utf8_lossy(&value));
        }
        other => println!("   Response: {:?}", other),
    }

    println!("🔍 Getting nonexistent key...");
    let response = manager.process_command(get_cmd3).await;
    println!("   Response: {:?}", response);

    // EXPIRE operation
    println!("\n⏰ Testing EXPIRE operation...");
    let expire_cmd = Command::Expire {
        key: Bytes::from("counter:visits"),
        ttl: 1800, // 30 minutes
    };

    println!("⏰ Setting TTL for counter:visits...");
    let response = manager.process_command(expire_cmd).await;
    println!("   Response: {:?}", response);

    // DELETE operation
    println!("\n🗑️  Testing DELETE operation...");
    let del_cmd = Command::Del {
        key: Bytes::from("user:1001"),
    };

    println!("🗑️  Deleting user:1001...");
    let response = manager.process_command(del_cmd).await;
    println!("   Response: {:?}", response);

    // Verify deletion
    let get_deleted = Command::Get {
        key: Bytes::from("user:1001"),
    };

    println!("🔍 Verifying deletion...");
    let response = manager.process_command(get_deleted).await;
    println!("   Response: {:?}", response);

    // Force WAL flush
    println!("\n💾 Flushing WAL to disk...");
    manager.flush_wal().await?;
    println!("✅ WAL flushed successfully!");

    // Get metrics
    println!("\n📊 Getting metrics...");
    let metrics = manager.get_metrics().await;
    println!("📊 Metrics: {}", serde_json::to_string_pretty(&metrics)?);

    // Simulate recovery by creating a new manager
    println!("\n🔄 Testing recovery...");
    println!("🔄 Creating new manager to test recovery...");

    let (recovery_manager, recovery_stats) = WALShardManager::new_with_recovery(
        2, // Same configuration
        1024 * 1024,
        EvictionConfig {
            max_capacity: 1000,
            ..Default::default()
        },
        Some(WALConfig {
            wal_dir: wal_dir.clone(),
            max_segment_size: 1024 * 1024,
            buffer_size: 4096,
            flush_interval_ms: 1000,
            sync_policy: SyncPolicy::Sync,
        }),
    )
    .await?;

    if let Some(stats) = recovery_stats {
        println!("📊 Recovery Stats: {:?}", stats);
        if stats.entries_recovered > 0 {
            println!(
                "✅ Successfully recovered {} operations!",
                stats.entries_recovered
            );
        }
    }

    // Test that recovered data is available
    println!("\n🔍 Testing recovered data...");
    let get_recovered = Command::Get {
        key: Bytes::from("user:1002"),
    };

    let response = recovery_manager.process_command(get_recovered).await;
    match response {
        Response::Value(value) => {
            println!("✅ Recovered data: {}", String::from_utf8_lossy(&value));
        }
        other => println!("❌ Recovery failed: {:?}", other),
    }

    // Graceful shutdown
    println!("\n🛑 Shutting down...");
    manager.shutdown().await?;
    recovery_manager.shutdown().await?;

    println!("✅ WAL persistence example completed successfully!");
    println!("\n📋 Summary:");
    println!("   • WAL-enabled shard manager created");
    println!("   • Basic operations (PUT/GET/DEL/EXPIRE) tested");
    println!("   • WAL persistence verified");
    println!("   • Recovery functionality tested");
    println!("   • Graceful shutdown completed");

    Ok(())
}
