//! Advanced Pipeline Example for CrabCache Phase 6.1
//! 
//! This example demonstrates the new advanced pipelining features:
//! - Parallel batch processing
//! - Adaptive batch sizing
//! - Smart command grouping
//! - Performance monitoring

use crabcache::protocol::{
    AdvancedPipelineProcessor, AdvancedPipelineConfig, Command, Response
};
use bytes::Bytes;
use std::time::Instant;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!("🚀 CrabCache Advanced Pipeline Example");
    println!("=====================================");
    
    // Create advanced pipeline configuration
    let config = AdvancedPipelineConfig {
        max_batch_size: 64,
        enable_parallel_parsing: true,
        enable_adaptive_sizing: true,
        enable_simd: true,
        enable_zero_copy: true,
        parser_threads: 4,
        metrics_interval_ms: 1000,
    };
    
    println!("📋 Configuration:");
    println!("   Max Batch Size: {}", config.max_batch_size);
    println!("   Parallel Parsing: {}", config.enable_parallel_parsing);
    println!("   Adaptive Sizing: {}", config.enable_adaptive_sizing);
    println!("   SIMD Enabled: {}", config.enable_simd);
    println!("   Zero-Copy: {}", config.enable_zero_copy);
    println!("   Parser Threads: {}", config.parser_threads);
    
    // Create advanced pipeline processor
    let processor = AdvancedPipelineProcessor::new(config);
    
    // Example 1: Basic batch processing
    println!("\n🧪 Example 1: Basic Batch Processing");
    let basic_batch = create_test_batch(16);
    let start_time = Instant::now();
    
    match processor.process_batch_advanced(&basic_batch).await {
        Ok(response_batch) => {
            let processing_time = start_time.elapsed();
            println!("✅ Processed {} commands in {:.2}ms", 
                    response_batch.responses.len(), 
                    processing_time.as_secs_f64() * 1000.0);
            
            let ops_per_second = response_batch.responses.len() as f64 / processing_time.as_secs_f64();
            println!("📊 Throughput: {:.0} ops/sec", ops_per_second);
        }
        Err(e) => println!("❌ Error: {}", e),
    }
    
    // Example 2: Large batch with parallel processing
    println!("\n🧪 Example 2: Large Batch with Parallel Processing");
    let large_batch = create_test_batch(128);
    let start_time = Instant::now();
    
    match processor.process_batch_advanced(&large_batch).await {
        Ok(response_batch) => {
            let processing_time = start_time.elapsed();
            println!("✅ Processed {} commands in {:.2}ms", 
                    response_batch.responses.len(), 
                    processing_time.as_secs_f64() * 1000.0);
            
            let ops_per_second = response_batch.responses.len() as f64 / processing_time.as_secs_f64();
            println!("📊 Throughput: {:.0} ops/sec", ops_per_second);
        }
        Err(e) => println!("❌ Error: {}", e),
    }
    
    // Example 3: Performance monitoring over time
    println!("\n🧪 Example 3: Performance Monitoring");
    
    for i in 1..=5 {
        let batch_size = 16 + (i * 8); // Increasing batch sizes
        let test_batch = create_test_batch(batch_size);
        let start_time = Instant::now();
        
        match processor.process_batch_advanced(&test_batch).await {
            Ok(_) => {
                let processing_time = start_time.elapsed();
                let ops_per_second = batch_size as f64 / processing_time.as_secs_f64();
                
                println!("   Batch {}: {} commands, {:.0} ops/sec, {:.2}ms", 
                        i, batch_size, ops_per_second, 
                        processing_time.as_secs_f64() * 1000.0);
            }
            Err(e) => println!("   Batch {}: Error - {}", i, e),
        }
        
        // Small delay between batches
        sleep(Duration::from_millis(100)).await;
    }
    
    // Example 4: Get performance metrics
    println!("\n📊 Example 4: Performance Metrics");
    let metrics = processor.get_metrics().await;
    
    println!("   Total Batches: {}", metrics.total_batches);
    println!("   Total Commands: {}", metrics.total_commands);
    println!("   Average Batch Size: {:.1}", metrics.avg_batch_size);
    println!("   Current Throughput: {:.0} ops/sec", metrics.current_throughput);
    println!("   Average Latency: {:.2}ms", metrics.avg_latency_ms);
    println!("   P99 Latency: {:.2}ms", metrics.p99_latency_ms);
    println!("   Parallel Efficiency: {:.1}%", metrics.parallel_efficiency * 100.0);
    println!("   SIMD Usage: {:.1}%", metrics.simd_usage_percent);
    println!("   Zero-Copy Operations: {:.1}%", metrics.zero_copy_percent);
    
    // Example 5: Stress test with varying batch sizes
    println!("\n🧪 Example 5: Adaptive Batch Size Stress Test");
    
    let batch_sizes = vec![4, 8, 16, 32, 64, 128];
    let mut best_performance = 0.0;
    let mut optimal_batch_size = 16;
    
    for &batch_size in &batch_sizes {
        let mut total_ops = 0;
        let mut total_time = 0.0;
        
        // Run multiple iterations for each batch size
        for _ in 0..5 {
            let test_batch = create_test_batch(batch_size);
            let start_time = Instant::now();
            
            if let Ok(response_batch) = processor.process_batch_advanced(&test_batch).await {
                let processing_time = start_time.elapsed();
                total_ops += response_batch.responses.len();
                total_time += processing_time.as_secs_f64();
            }
        }
        
        let avg_ops_per_second = total_ops as f64 / total_time;
        println!("   Batch Size {}: {:.0} ops/sec (avg over 5 runs)", 
                batch_size, avg_ops_per_second);
        
        if avg_ops_per_second > best_performance {
            best_performance = avg_ops_per_second;
            optimal_batch_size = batch_size;
        }
    }
    
    println!("\n🏆 Optimal Configuration Found:");
    println!("   Best Batch Size: {}", optimal_batch_size);
    println!("   Best Performance: {:.0} ops/sec", best_performance);
    
    // Example 6: Mixed workload simulation
    println!("\n🧪 Example 6: Mixed Workload Simulation");
    
    let workloads = vec![
        ("Read Heavy", create_read_heavy_batch(32)),
        ("Write Heavy", create_write_heavy_batch(32)),
        ("Mixed", create_mixed_batch(32)),
    ];
    
    for (workload_name, batch_data) in workloads {
        let start_time = Instant::now();
        
        match processor.process_batch_advanced(&batch_data).await {
            Ok(response_batch) => {
                let processing_time = start_time.elapsed();
                let ops_per_second = response_batch.responses.len() as f64 / processing_time.as_secs_f64();
                
                println!("   {}: {:.0} ops/sec, {:.2}ms", 
                        workload_name, ops_per_second, 
                        processing_time.as_secs_f64() * 1000.0);
            }
            Err(e) => println!("   {}: Error - {}", workload_name, e),
        }
    }
    
    // Final metrics
    println!("\n📈 Final Performance Summary:");
    let final_metrics = processor.get_metrics().await;
    
    println!("   Total Operations Processed: {}", final_metrics.total_commands);
    println!("   Average Throughput: {:.0} ops/sec", final_metrics.current_throughput);
    println!("   Average Latency: {:.2}ms", final_metrics.avg_latency_ms);
    
    // Performance assessment
    let target_ops_per_second = 300_000.0;
    let performance_ratio = final_metrics.current_throughput / target_ops_per_second;
    
    println!("\n🎯 Performance Assessment:");
    println!("   Target: {:.0} ops/sec", target_ops_per_second);
    println!("   Achieved: {:.0} ops/sec", final_metrics.current_throughput);
    println!("   Ratio: {:.1}% of target", performance_ratio * 100.0);
    
    if performance_ratio >= 1.0 {
        println!("   Status: 🎉 TARGET EXCEEDED!");
    } else if performance_ratio >= 0.8 {
        println!("   Status: ✅ GOOD PERFORMANCE");
    } else if performance_ratio >= 0.6 {
        println!("   Status: 🟡 MODERATE PERFORMANCE");
    } else {
        println!("   Status: 🔴 NEEDS OPTIMIZATION");
    }
    
    println!("\n✨ Advanced Pipeline Example Complete!");
    
    Ok(())
}

/// Create a test batch with specified number of commands
fn create_test_batch(command_count: usize) -> Vec<u8> {
    let mut batch = Vec::new();
    
    for i in 0..command_count {
        let command = match i % 4 {
            0 => format!("GET key_{}\n", i),
            1 => format!("PUT key_{} value_{}\n", i, i),
            2 => format!("DEL key_{}\n", i),
            3 => "PING\n".to_string(),
            _ => unreachable!(),
        };
        batch.extend_from_slice(command.as_bytes());
    }
    
    batch
}

/// Create a read-heavy workload batch
fn create_read_heavy_batch(command_count: usize) -> Vec<u8> {
    let mut batch = Vec::new();
    
    for i in 0..command_count {
        let command = if i % 10 < 8 {
            // 80% reads
            format!("GET key_{}\n", i % 1000)
        } else {
            // 20% writes
            format!("PUT key_{} value_{}\n", i % 1000, i)
        };
        batch.extend_from_slice(command.as_bytes());
    }
    
    batch
}

/// Create a write-heavy workload batch
fn create_write_heavy_batch(command_count: usize) -> Vec<u8> {
    let mut batch = Vec::new();
    
    for i in 0..command_count {
        let command = if i % 10 < 2 {
            // 20% reads
            format!("GET key_{}\n", i % 1000)
        } else {
            // 80% writes
            format!("PUT key_{} value_{}\n", i % 1000, i)
        };
        batch.extend_from_slice(command.as_bytes());
    }
    
    batch
}

/// Create a mixed workload batch
fn create_mixed_batch(command_count: usize) -> Vec<u8> {
    let mut batch = Vec::new();
    
    for i in 0..command_count {
        let command = match i % 10 {
            0..=4 => format!("GET key_{}\n", i % 1000),      // 50% reads
            5..=7 => format!("PUT key_{} value_{}\n", i % 1000, i), // 30% writes
            8 => format!("DEL key_{}\n", i % 1000),          // 10% deletes
            9 => "PING\n".to_string(),                       // 10% pings
            _ => unreachable!(),
        };
        batch.extend_from_slice(command.as_bytes());
    }
    
    batch
}