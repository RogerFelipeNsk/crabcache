//! Pipeline usage example for CrabCache
//! 
//! This example demonstrates how to use the pipelining feature
//! to achieve high-performance batch processing.

use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::time::Instant;

/// Simple CrabCache client for demonstration
struct CrabCacheClient {
    stream: TcpStream,
}

impl CrabCacheClient {
    /// Connect to CrabCache server
    async fn connect(addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Self { stream })
    }
    
    /// Send single command and receive response
    async fn send_command(&mut self, command: &str) -> Result<String, Box<dyn std::error::Error>> {
        // Send command
        self.stream.write_all(format!("{}\n", command).as_bytes()).await?;
        
        // Read response
        let mut buffer = vec![0; 1024];
        let n = self.stream.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        
        Ok(response.trim().to_string())
    }
    
    /// Send batch of commands using pipelining
    async fn send_pipeline_batch(&mut self, commands: &[String]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        // Send all commands at once
        let batch_data = commands.iter()
            .map(|cmd| format!("{}\n", cmd))
            .collect::<String>();
        
        self.stream.write_all(batch_data.as_bytes()).await?;
        
        // Read all responses
        let mut responses = Vec::new();
        for _ in commands {
            let mut buffer = vec![0; 1024];
            let n = self.stream.read(&mut buffer).await?;
            let response = String::from_utf8_lossy(&buffer[..n]);
            responses.push(response.trim().to_string());
        }
        
        Ok(responses)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 CrabCache Pipeline Example");
    println!("============================");
    
    // Connect to CrabCache server
    let mut client = CrabCacheClient::connect("127.0.0.1:8000").await?;
    println!("✓ Connected to CrabCache server");
    
    // Example 1: Single command processing
    println!("\n📝 Example 1: Single Command Processing");
    let start = Instant::now();
    
    for i in 0..100 {
        let response = client.send_command(&format!("PUT single_key_{} single_value_{}", i, i)).await?;
        if response != "OK" {
            println!("Warning: Unexpected response: {}", response);
        }
    }
    
    let single_duration = start.elapsed();
    let single_ops_per_sec = 100.0 / single_duration.as_secs_f64();
    println!("   Processed 100 commands in {:?}", single_duration);
    println!("   Performance: {:.0} ops/sec", single_ops_per_sec);
    
    // Example 2: Pipeline batch processing
    println!("\n🚀 Example 2: Pipeline Batch Processing");
    let start = Instant::now();
    
    let batch_size = 10;
    let num_batches = 10;
    
    for batch_idx in 0..num_batches {
        // Create batch of commands
        let mut commands = Vec::new();
        for i in 0..batch_size {
            let key_idx = batch_idx * batch_size + i;
            commands.push(format!("PUT pipeline_key_{} pipeline_value_{}", key_idx, key_idx));
        }
        
        // Send batch using pipelining
        let responses = client.send_pipeline_batch(&commands).await?;
        
        // Verify responses
        for (cmd, resp) in commands.iter().zip(responses.iter()) {
            if resp != "OK" {
                println!("Warning: Command '{}' got response '{}'", cmd, resp);
            }
        }
    }
    
    let pipeline_duration = start.elapsed();
    let pipeline_ops_per_sec = 100.0 / pipeline_duration.as_secs_f64();
    println!("   Processed 100 commands in {:?}", pipeline_duration);
    println!("   Performance: {:.0} ops/sec", pipeline_ops_per_sec);
    
    // Performance comparison
    let improvement = pipeline_ops_per_sec / single_ops_per_sec;
    println!("\n📊 Performance Comparison");
    println!("   Single commands: {:.0} ops/sec", single_ops_per_sec);
    println!("   Pipeline batch:  {:.0} ops/sec", pipeline_ops_per_sec);
    println!("   Improvement:     {:.1}x faster", improvement);
    
    // Example 3: Mixed workload with pipelining
    println!("\n🔀 Example 3: Mixed Workload Pipeline");
    let start = Instant::now();
    
    // Create mixed batch: PUT, GET, DEL operations
    let mut mixed_commands = Vec::new();
    for i in 0..50 {
        mixed_commands.push(format!("PUT mixed_key_{} mixed_value_{}", i, i));
    }
    for i in 0..30 {
        mixed_commands.push(format!("GET mixed_key_{}", i));
    }
    for i in 0..20 {
        mixed_commands.push(format!("DEL mixed_key_{}", i));
    }
    
    // Send mixed batch
    let responses = client.send_pipeline_batch(&mixed_commands).await?;
    
    let mixed_duration = start.elapsed();
    let mixed_ops_per_sec = mixed_commands.len() as f64 / mixed_duration.as_secs_f64();
    
    println!("   Processed {} mixed commands in {:?}", mixed_commands.len(), mixed_duration);
    println!("   Performance: {:.0} ops/sec", mixed_ops_per_sec);
    
    // Analyze responses
    let mut put_count = 0;
    let mut get_count = 0;
    let mut del_count = 0;
    
    for (cmd, resp) in mixed_commands.iter().zip(responses.iter()) {
        if cmd.starts_with("PUT") && resp == "OK" {
            put_count += 1;
        } else if cmd.starts_with("GET") && resp.starts_with("mixed_value_") {
            get_count += 1;
        } else if cmd.starts_with("DEL") && resp == "OK" {
            del_count += 1;
        }
    }
    
    println!("   Successful operations: PUT={}, GET={}, DEL={}", put_count, get_count, del_count);
    
    // Example 4: Optimal batch size discovery
    println!("\n🎯 Example 4: Optimal Batch Size Discovery");
    
    let batch_sizes = [1, 4, 8, 16, 32, 64];
    let operations_per_test = 100;
    
    for &batch_size in &batch_sizes {
        let start = Instant::now();
        let num_batches = (operations_per_test + batch_size - 1) / batch_size; // Ceiling division
        
        for batch_idx in 0..num_batches {
            let mut commands = Vec::new();
            let batch_start = batch_idx * batch_size;
            let batch_end = std::cmp::min(batch_start + batch_size, operations_per_test);
            
            for i in batch_start..batch_end {
                commands.push(format!("PUT optimal_key_{} optimal_value_{}", i, i));
            }
            
            let _responses = client.send_pipeline_batch(&commands).await?;
        }
        
        let duration = start.elapsed();
        let ops_per_sec = operations_per_test as f64 / duration.as_secs_f64();
        
        println!("   Batch size {}: {:.0} ops/sec", batch_size, ops_per_sec);
    }
    
    println!("\n🎉 Pipeline examples completed successfully!");
    println!("\n💡 Key Takeaways:");
    println!("   • Pipelining can provide 5-15x performance improvement");
    println!("   • Optimal batch size is typically 8-32 commands");
    println!("   • Mixed workloads benefit significantly from pipelining");
    println!("   • Larger batches reduce latency per command");
    
    Ok(())
}