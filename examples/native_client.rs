//! Native client example demonstrating Phase 3 performance features
//!
//! This example shows how to use the high-performance native client
//! with binary protocol, connection pooling, and pipelining.

use crabcache::client::{ClientConfig, NativeClient};
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 CrabCache Native Client - Phase 3 Performance Demo");
    println!("====================================================");

    // Configure high-performance client
    let config = ClientConfig {
        address: "127.0.0.1:7001".to_string(),
        connection_pool_size: 20,
        connection_timeout: Duration::from_secs(5),
        pipeline_batch_size: 1000,
        enable_pipelining: true,
        force_binary_protocol: true, // Phase 3: Force binary protocol
    };

    println!("📡 Connecting to CrabCache server at {}...", config.address);

    // Create native client
    let client = match NativeClient::new(config).await {
        Ok(client) => {
            println!("✅ Connected successfully!");
            client
        }
        Err(e) => {
            println!("❌ Failed to connect: {}", e);
            println!("💡 Make sure CrabCache server is running on port 7001");
            return Ok(());
        }
    };

    // Test basic operations
    println!("\n🔧 Testing basic operations...");
    test_basic_operations(&client).await?;

    // Test pipelining
    println!("\n🚀 Testing pipeline operations...");
    test_pipeline_operations(&client).await?;

    // Performance benchmark
    println!("\n⚡ Running performance benchmark...");
    run_performance_benchmark(&client).await?;

    // Show metrics
    println!("\n📊 Client Metrics:");
    show_metrics(&client);

    println!("\n🎉 Native client demo completed!");
    Ok(())
}

async fn test_basic_operations(client: &NativeClient) -> Result<(), Box<dyn std::error::Error>> {
    // Test PING
    print!("  PING... ");
    client.ping().await?;
    println!("✅ PONG");

    // Test PUT
    print!("  PUT key1 = 'Hello, CrabCache!'... ");
    client.put(b"key1", b"Hello, CrabCache!").await?;
    println!("✅ OK");

    // Test GET
    print!("  GET key1... ");
    if let Some(value) = client.get(b"key1").await? {
        let value_str = String::from_utf8_lossy(&value);
        println!("✅ '{}'", value_str);
    } else {
        println!("❌ Not found");
    }

    // Test PUT with TTL
    print!("  PUT key2 = 'Temporary' (TTL: 60s)... ");
    client.put_with_ttl(b"key2", b"Temporary", 60).await?;
    println!("✅ OK");

    // Test EXPIRE
    print!("  EXPIRE key1 300... ");
    let expired = client.expire(b"key1", 300).await?;
    println!("✅ {}", if expired { "Set" } else { "Not found" });

    // Test DEL
    print!("  DEL key2... ");
    let deleted = client.del(b"key2").await?;
    println!("✅ {}", if deleted { "Deleted" } else { "Not found" });

    Ok(())
}

async fn test_pipeline_operations(client: &NativeClient) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    // Create a pipeline with multiple operations
    let mut pipeline = client.pipeline().await;

    // Add multiple operations to the pipeline
    pipeline
        .put(b"batch_key1", b"value1")
        .put(b"batch_key2", b"value2")
        .put(b"batch_key3", b"value3")
        .get(b"batch_key1")
        .get(b"batch_key2")
        .get(b"batch_key3")
        .ping()
        .del(b"batch_key1")
        .del(b"batch_key2")
        .del(b"batch_key3");

    println!("  Executing {} commands in pipeline...", pipeline.len());

    // Execute the pipeline
    let responses = pipeline.execute().await?;

    let elapsed = start_time.elapsed();
    println!(
        "  ✅ Pipeline completed in {:.2}ms",
        elapsed.as_secs_f64() * 1000.0
    );
    println!("  📊 {} responses received", responses.len());

    // Analyze responses
    let mut success_count = 0;
    for (i, response) in responses.iter().enumerate() {
        match response {
            crabcache::protocol::commands::Response::Ok => success_count += 1,
            crabcache::protocol::commands::Response::Pong => success_count += 1,
            crabcache::protocol::commands::Response::Value(_) => success_count += 1,
            crabcache::protocol::commands::Response::Null => success_count += 1,
            crabcache::protocol::commands::Response::Error(e) => {
                println!("    ❌ Command {}: Error - {}", i + 1, e);
            }
            _ => {}
        }
    }

    println!(
        "  ✅ {}/{} commands successful",
        success_count,
        responses.len()
    );

    Ok(())
}

async fn run_performance_benchmark(
    client: &NativeClient,
) -> Result<(), Box<dyn std::error::Error>> {
    const OPERATIONS: usize = 1000;
    const BATCH_SIZE: usize = 100;

    println!(
        "  Running {} operations in batches of {}...",
        OPERATIONS, BATCH_SIZE
    );

    let start_time = Instant::now();
    let mut total_operations = 0;

    // Run operations in batches
    for batch in 0..(OPERATIONS / BATCH_SIZE) {
        let mut pipeline = client.pipeline().await;

        // Add operations to pipeline
        for i in 0..BATCH_SIZE {
            let key = format!("bench_key_{}", batch * BATCH_SIZE + i);
            let value = format!("bench_value_{}", batch * BATCH_SIZE + i);

            pipeline
                .put(key.as_bytes(), value.as_bytes())
                .get(key.as_bytes());
        }

        // Execute batch
        let responses = pipeline.execute().await?;
        total_operations += responses.len();

        // Small delay to avoid overwhelming the server
        if batch % 10 == 0 {
            sleep(Duration::from_millis(1)).await;
        }
    }

    let elapsed = start_time.elapsed();
    let ops_per_sec = total_operations as f64 / elapsed.as_secs_f64();
    let avg_latency = elapsed.as_secs_f64() * 1000.0 / total_operations as f64;

    println!("  📊 Performance Results:");
    println!("    Total operations: {}", total_operations);
    println!("    Total time: {:.2}ms", elapsed.as_secs_f64() * 1000.0);
    println!("    Throughput: {:.0} ops/sec", ops_per_sec);
    println!("    Average latency: {:.2}ms", avg_latency);

    // Compare with Phase 2 baseline
    const PHASE2_BASELINE: f64 = 5092.0;
    let improvement = (ops_per_sec / PHASE2_BASELINE - 1.0) * 100.0;

    if improvement > 0.0 {
        println!("    🚀 {:.1}% improvement over Phase 2!", improvement);
    } else {
        println!(
            "    📉 {:.1}% slower than Phase 2 baseline",
            improvement.abs()
        );
    }

    Ok(())
}

fn show_metrics(client: &NativeClient) {
    let metrics = client.metrics();
    let pool_metrics = client.pool_metrics();

    println!("  Client Metrics:");
    println!("    Total requests: {}", metrics.total_requests);
    println!("    Success rate: {:.1}%", metrics.success_rate());
    println!("    Average latency: {:.2}ms", metrics.average_latency_ms());
    println!(
        "    Binary protocol usage: {:.1}%",
        metrics.binary_protocol_rate()
    );
    println!("    Pipeline requests: {}", metrics.pipeline_requests);

    println!("  Connection Pool Metrics:");
    println!(
        "    Active connections: {}",
        pool_metrics.active_connections
    );
    println!("    Idle connections: {}", pool_metrics.idle_connections);
    println!("    Total created: {}", pool_metrics.total_created);
    println!("    Pool hits: {}", pool_metrics.pool_hits);
    println!("    Pool misses: {}", pool_metrics.pool_misses);
    println!(
        "    Health check failures: {}",
        pool_metrics.health_check_failures
    );
}
