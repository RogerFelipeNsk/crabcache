//! Simple client example for testing CrabCache

use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Connecting to CrabCache server...");
    
    let mut stream = TcpStream::connect("127.0.0.1:7000").await?;
    println!("Connected!");
    
    // Test PING command
    println!("Sending PING...");
    stream.write_all(b"PING\r\n").await?;
    
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    println!("Response: {}", response.trim());
    
    // Test PUT command
    println!("Sending PUT test_key test_value...");
    stream.write_all(b"PUT test_key test_value\r\n").await?;
    
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    println!("Response: {}", response.trim());
    
    // Test GET command
    println!("Sending GET test_key...");
    stream.write_all(b"GET test_key\r\n").await?;
    
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    println!("Response: {}", response.trim());
    
    // Test DEL command
    println!("Sending DEL test_key...");
    stream.write_all(b"DEL test_key\r\n").await?;
    
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    println!("Response: {}", response.trim());
    
    // Test STATS command
    println!("Sending STATS...");
    stream.write_all(b"STATS\r\n").await?;
    
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    println!("Response: {}", response.trim());
    
    println!("Client test completed!");
    Ok(())
}