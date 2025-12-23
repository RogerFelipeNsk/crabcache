//! Security system example for CrabCache
//! 
//! This example demonstrates how to use the security features:
//! - Authentication with tokens
//! - Rate limiting
//! - IP filtering

use crabcache::security::{AuthManager, RateLimiter, IpFilter, SecurityManager, SecurityContext};
use std::net::IpAddr;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!("🔐 CrabCache Security System Example");
    println!("=====================================\n");
    
    // 1. Authentication Example
    println!("1. Authentication System");
    println!("------------------------");
    
    let auth_manager = AuthManager::with_token("secret123".to_string(), "admin".to_string());
    
    // Test valid token
    let result = auth_manager.authenticate(Some("secret123"));
    println!("✅ Valid token: {:?}", result);
    
    // Test invalid token
    let result = auth_manager.authenticate(Some("invalid"));
    println!("❌ Invalid token: {:?}", result);
    
    // Test no token
    let result = auth_manager.authenticate(None);
    println!("❌ No token: {:?}", result);
    
    // Add more tokens
    auth_manager.add_token("user456".to_string(), "user1".to_string()).await;
    let result = auth_manager.authenticate(Some("user456"));
    println!("✅ New token: {:?}", result);
    
    println!("📊 Total tokens: {}\n", auth_manager.token_count().await);
    
    // 2. Rate Limiting Example
    println!("2. Rate Limiting System");
    println!("-----------------------");
    
    let rate_limiter = RateLimiter::new(5, 10); // 5 req/sec, 10 burst
    
    // Test burst capacity
    println!("Testing burst capacity (10 requests):");
    for i in 1..=10 {
        let result = rate_limiter.check_rate("client1").await;
        println!("  Request {}: {:?}", i, result);
    }
    
    // This should be rate limited
    let result = rate_limiter.check_rate("client1").await;
    println!("  Request 11: {:?} (should be rate limited)", result);
    
    // Different client should have its own bucket
    let result = rate_limiter.check_rate("client2").await;
    println!("  Client2 Request 1: {:?} (should be allowed)", result);
    
    println!("📊 Active buckets: {}\n", rate_limiter.active_buckets().await);
    
    // 3. IP Filtering Example
    println!("3. IP Filtering System");
    println!("----------------------");
    
    let allowed_ips = vec![
        "127.0.0.1".to_string(),
        "192.168.1.0/24".to_string(),
        "10.0.0.0/8".to_string(),
    ];
    
    let ip_filter = IpFilter::new(allowed_ips, false)?;
    
    // Test allowed IPs
    let test_ips = [
        ("127.0.0.1", "localhost"),
        ("192.168.1.100", "local network"),
        ("10.1.2.3", "private network"),
        ("8.8.8.8", "public internet"),
        ("172.16.1.1", "different private network"),
    ];
    
    for (ip_str, description) in &test_ips {
        let ip: IpAddr = ip_str.parse()?;
        let result = ip_filter.check_ip(ip);
        let status = if result.is_allowed() { "✅ ALLOWED" } else { "❌ BLOCKED" };
        println!("  {} ({}): {}", ip_str, description, status);
    }
    
    println!("📊 Allowed IPs: {}, Networks: {}\n", 
             ip_filter.allowed_ip_count(), 
             ip_filter.allowed_network_count());
    
    // 4. Complete Security Manager Example
    println!("4. Complete Security Manager");
    println!("----------------------------");
    
    let security_manager = SecurityManager::new(
        Some(auth_manager),
        Some(rate_limiter),
        Some(ip_filter),
    );
    
    // Test connection from allowed IP
    let mut context = SecurityContext::new("127.0.0.1".parse()?);
    let result = security_manager.check_connection(&context).await;
    println!("Connection from 127.0.0.1: {:?}", result);
    
    // Test authentication
    let result = security_manager.authenticate_command(&mut context, Some("secret123"));
    println!("Authentication with valid token: {:?}", result);
    println!("Context after auth: authenticated={}, user={:?}", 
             context.authenticated, context.user_id);
    
    // Test connection from blocked IP
    let context = SecurityContext::new("8.8.8.8".parse()?);
    let result = security_manager.check_connection(&context).await;
    println!("Connection from 8.8.8.8: {:?}", result);
    
    println!("\n🎉 Security system example completed!");
    println!("💡 Use environment variables to configure security in production:");
    println!("   CRABCACHE_ENABLE_AUTH=true");
    println!("   CRABCACHE_AUTH_TOKEN=your-secret-token");
    println!("   CRABCACHE_ENABLE_RATE_LIMIT=true");
    println!("   CRABCACHE_MAX_REQUESTS_PER_SECOND=1000");
    println!("   CRABCACHE_ALLOWED_IPS=127.0.0.1,192.168.1.0/24");
    
    Ok(())
}