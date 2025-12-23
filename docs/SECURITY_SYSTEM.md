# CrabCache Security System

## Overview

CrabCache includes a comprehensive security system designed to protect your cache server in production environments. The security system provides multiple layers of protection:

- **Authentication**: Token-based authentication system
- **Rate Limiting**: Token bucket algorithm for request rate limiting
- **IP Filtering**: Allow/block specific IP addresses or networks
- **Connection Security**: Integrated security checks for all connections

## Features

### 🔐 Authentication System

- **Token-based authentication**: Simple and secure token validation
- **Multiple tokens**: Support for multiple authentication tokens
- **User identification**: Each token can be associated with a user ID
- **Optional authentication**: Can be disabled for development environments

### 🚦 Rate Limiting

- **Token bucket algorithm**: Industry-standard rate limiting
- **Per-client limits**: Each client IP has its own rate limit bucket
- **Configurable rates**: Customizable requests per second and burst capacity
- **Automatic cleanup**: Old rate limit buckets are automatically cleaned up

### 🌐 IP Filtering

- **IP whitelist**: Allow only specific IP addresses
- **CIDR support**: Support for network ranges (e.g., 192.168.1.0/24)
- **IPv4 and IPv6**: Full support for both IP versions
- **Flexible configuration**: Allow all IPs when no restrictions are configured

## Configuration

### Environment Variables

```bash
# Authentication
CRABCACHE_ENABLE_AUTH=true
CRABCACHE_AUTH_TOKEN=your-secret-token-here

# Rate Limiting
CRABCACHE_ENABLE_RATE_LIMIT=true
CRABCACHE_MAX_REQUESTS_PER_SECOND=1000
CRABCACHE_BURST_CAPACITY=100

# IP Filtering
CRABCACHE_ALLOWED_IPS=127.0.0.1,192.168.1.0/24,10.0.0.0/8

# Connection Settings
CRABCACHE_MAX_CONNECTIONS=1000
CRABCACHE_CONNECTION_TIMEOUT=30
CRABCACHE_MAX_COMMAND_SIZE=1048576
```

### TOML Configuration

```toml
[security]
# Enable authentication (requires auth_token)
enable_auth = false
# Authentication token (set via environment variable)
# auth_token = "your-secret-token-here"
# Allowed client IPs (empty = allow all)
allowed_ips = ["127.0.0.1", "192.168.1.0/24"]
# Enable TLS (requires cert and key files)
enable_tls = false
# Maximum command size in bytes
max_command_size = 1048576

[rate_limiting]
# Enable rate limiting
enabled = false
# Maximum requests per second per client
max_requests_per_second = 1000
# Burst capacity
burst_capacity = 100
# Rate limit window in seconds
window_seconds = 1

[connection]
# Maximum concurrent connections
max_connections = 1000
# Connection timeout in seconds
connection_timeout_seconds = 30
# Keep-alive timeout in seconds
keepalive_timeout_seconds = 300
# TCP nodelay for low latency
tcp_nodelay = true
```

## Usage Examples

### Basic Authentication

```rust
use crabcache::security::AuthManager;

// Create auth manager with a token
let auth_manager = AuthManager::with_token(
    "secret123".to_string(), 
    "admin".to_string()
);

// Authenticate requests
let result = auth_manager.authenticate(Some("secret123"));
match result {
    AuthResult::Authenticated(user_id) => {
        println!("User {} authenticated", user_id);
    }
    AuthResult::InvalidToken => {
        println!("Invalid token");
    }
    AuthResult::Unauthenticated => {
        println!("No token provided");
    }
}
```

### Rate Limiting

```rust
use crabcache::security::RateLimiter;

// Create rate limiter: 100 req/sec, 50 burst capacity
let rate_limiter = RateLimiter::new(100, 50);

// Check if request is allowed
let result = rate_limiter.check_rate("client_ip").await;
match result {
    RateLimitResult::Allowed => {
        // Process request
    }
    RateLimitResult::RateLimited => {
        // Reject request
    }
}
```

### IP Filtering

```rust
use crabcache::security::IpFilter;

// Create IP filter with allowed IPs/networks
let allowed_ips = vec![
    "127.0.0.1".to_string(),
    "192.168.1.0/24".to_string(),
    "10.0.0.0/8".to_string(),
];

let ip_filter = IpFilter::new(allowed_ips, false)?;

// Check if IP is allowed
let client_ip = "192.168.1.100".parse()?;
let result = ip_filter.check_ip(client_ip);
match result {
    IpFilterResult::Allowed => {
        // Allow connection
    }
    IpFilterResult::Blocked => {
        // Block connection
    }
}
```

### Complete Security Manager

```rust
use crabcache::security::{SecurityManager, SecurityContext};

// Create security manager with all features
let security_manager = SecurityManager::new(
    Some(auth_manager),
    Some(rate_limiter),
    Some(ip_filter),
);

// Check connection security
let mut context = SecurityContext::new(client_ip);
let result = security_manager.check_connection(&context).await;

if result.is_allowed() {
    // Authenticate command
    let auth_result = security_manager.authenticate_command(
        &mut context, 
        Some("auth_token")
    );
    
    if auth_result.is_allowed() {
        // Process command
    }
}
```

## Security Best Practices

### Production Deployment

1. **Always enable authentication** in production:
   ```bash
   CRABCACHE_ENABLE_AUTH=true
   CRABCACHE_AUTH_TOKEN=$(openssl rand -hex 32)
   ```

2. **Use strong tokens**: Generate cryptographically secure tokens:
   ```bash
   # Generate a secure token
   openssl rand -hex 32
   ```

3. **Restrict IP access**: Only allow known client IPs:
   ```bash
   CRABCACHE_ALLOWED_IPS=10.0.0.0/8,172.16.0.0/12,192.168.0.0/16
   ```

4. **Enable rate limiting**: Protect against abuse:
   ```bash
   CRABCACHE_ENABLE_RATE_LIMIT=true
   CRABCACHE_MAX_REQUESTS_PER_SECOND=1000
   ```

5. **Limit connections**: Prevent resource exhaustion:
   ```bash
   CRABCACHE_MAX_CONNECTIONS=1000
   CRABCACHE_CONNECTION_TIMEOUT=30
   ```

### Token Management

- **Rotate tokens regularly**: Change authentication tokens periodically
- **Use different tokens per environment**: Separate tokens for dev/staging/prod
- **Store tokens securely**: Use environment variables or secure key management
- **Monitor authentication failures**: Log and alert on failed authentication attempts

### Network Security

- **Use private networks**: Deploy CrabCache in private network segments
- **Firewall rules**: Configure firewall rules to restrict access
- **TLS encryption**: Enable TLS for encrypted communication (future feature)
- **VPN access**: Require VPN access for remote connections

### Monitoring and Alerting

- **Monitor rate limits**: Track rate limiting events
- **Authentication logs**: Monitor authentication success/failure rates
- **Connection metrics**: Track connection counts and patterns
- **Security alerts**: Set up alerts for security events

## Performance Impact

The security system is designed for minimal performance impact:

- **Authentication**: O(1) token lookup using HashMap
- **Rate limiting**: O(1) token bucket operations
- **IP filtering**: O(1) IP lookup for exact matches, O(n) for CIDR ranges
- **Memory usage**: Minimal memory overhead per client
- **Cleanup**: Automatic cleanup of old rate limit buckets

## Error Handling

The security system provides graceful error handling:

- **Invalid configuration**: Falls back to secure defaults
- **Authentication failures**: Clear error messages
- **Rate limit exceeded**: Proper HTTP-like error responses
- **IP blocking**: Immediate connection termination
- **Resource limits**: Graceful handling of memory/connection limits

## Future Enhancements

Planned security features for future releases:

- **TLS/SSL support**: Encrypted communication
- **JWT tokens**: JSON Web Token support
- **Role-based access**: Different permission levels
- **Audit logging**: Comprehensive security audit logs
- **Brute force protection**: Advanced attack detection
- **API key management**: REST API for token management

## Testing

Run the security example to test all features:

```bash
cargo run --example security_example
```

Run security-specific tests:

```bash
cargo test security
```

## Troubleshooting

### Common Issues

1. **Authentication always fails**:
   - Check if `CRABCACHE_ENABLE_AUTH=true`
   - Verify `CRABCACHE_AUTH_TOKEN` is set correctly
   - Ensure token is passed in requests

2. **Rate limiting too aggressive**:
   - Increase `CRABCACHE_MAX_REQUESTS_PER_SECOND`
   - Increase `CRABCACHE_BURST_CAPACITY`
   - Check if multiple clients share the same IP

3. **IP blocking issues**:
   - Verify IP addresses in `CRABCACHE_ALLOWED_IPS`
   - Check CIDR notation (e.g., `192.168.1.0/24`)
   - Test with `allowed_ips = []` to allow all IPs

4. **Connection refused**:
   - Check if IP is in allowed list
   - Verify rate limits aren't exceeded
   - Check connection limits

### Debug Mode

Enable debug logging to troubleshoot security issues:

```bash
RUST_LOG=debug cargo run
```

This will show detailed security decision logs for authentication, rate limiting, and IP filtering.