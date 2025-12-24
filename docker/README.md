# CrabCache Docker Image 🦀

[![Docker Pulls](https://img.shields.io/docker/pulls/rogerfelipensk/crabcache)](https://hub.docker.com/r/rogerfelipensk/crabcache)
[![Docker Image Size](https://img.shields.io/docker/image-size/rogerfelipensk/crabcache/latest)](https://hub.docker.com/r/rogerfelipensk/crabcache)
[![Version](https://img.shields.io/badge/version-0.0.1-green.svg)](#version)

**CrabCache** is an ultra-high-performance cache server written in Rust - **Educational project for learning purposes**.

> **⚠️ Educational Notice**: This project was developed for educational purposes through VibeCoding. Performance claims and benchmarks should be independently validated before production use.

## 🚀 Quick Start

```bash
# Run CrabCache with default settings (Educational version)
docker run -p 8000:8000 rogerfelipensk/crabcache:0.0.1

# Test the server
echo "PING" | nc localhost 8000
# Response: PONG
```

## 📊 Performance Highlights

- **219,540+ ops/sec** with pipelining (batch size 16)
- **5.9x faster** than Redis with pipelining
- **Sub-millisecond latency** (0.00ms average, 0.02ms P99)
- **12.8x improvement** over single command processing

## 🔧 Configuration Options

### Basic Usage

```bash
# Default configuration
docker run -p 8000:8000 -p 8001:8001 crabcache/crabcache:latest
```

### With Persistence (WAL)

```bash
docker run -p 8000:8000 -p 8001:8001 \
  -e CRABCACHE_ENABLE_WAL=true \
  -e CRABCACHE_WAL_SYNC_POLICY=async \
  -v /data/wal:/app/data/wal \
  crabcache/crabcache:latest
```

### With Security

```bash
docker run -p 8000:8000 -p 8001:8001 \
  -e CRABCACHE_ENABLE_AUTH=true \
  -e CRABCACHE_AUTH_TOKEN=your-secret-token \
  -e CRABCACHE_ENABLE_RATE_LIMIT=true \
  -e CRABCACHE_ALLOWED_IPS=192.168.1.0/24 \
  crabcache/crabcache:latest
```

### High-Performance Configuration

```bash
docker run -p 8000:8000 -p 8001:8001 \
  -e CRABCACHE_ENABLE_PIPELINING=true \
  -e CRABCACHE_MAX_BATCH_SIZE=32 \
  -e CRABCACHE_MAX_CONNECTIONS=2000 \
  --memory=4g \
  crabcache/crabcache:latest
```

## 🌐 Environment Variables

### Server Configuration
- `CRABCACHE_BIND_ADDR` - Bind address (default: `0.0.0.0`)
- `CRABCACHE_PORT` - Server port (default: `8000`)
- `CRABCACHE_MAX_CONNECTIONS` - Max concurrent connections (default: `1000`)

### Performance Settings
- `CRABCACHE_ENABLE_PIPELINING` - Enable pipelining (default: `true`)
- `CRABCACHE_MAX_BATCH_SIZE` - Pipeline batch size (default: `16`)
- `CRABCACHE_PIPELINE_BUFFER_SIZE` - Pipeline buffer size (default: `16384`)

### Persistence (WAL)
- `CRABCACHE_ENABLE_WAL` - Enable Write-Ahead Log (default: `false`)
- `CRABCACHE_WAL_DIR` - WAL directory (default: `./data/wal`)
- `CRABCACHE_WAL_SYNC_POLICY` - Sync policy: `none`, `async`, `sync` (default: `async`)

### Security
- `CRABCACHE_ENABLE_AUTH` - Enable authentication (default: `false`)
- `CRABCACHE_AUTH_TOKEN` - Authentication token
- `CRABCACHE_ALLOWED_IPS` - Allowed client IPs (comma-separated)
- `CRABCACHE_ENABLE_RATE_LIMIT` - Enable rate limiting (default: `false`)
- `CRABCACHE_MAX_REQUESTS_PER_SECOND` - Rate limit (default: `1000`)

### Logging
- `RUST_LOG` - Log level: `trace`, `debug`, `info`, `warn`, `error` (default: `info`)
- `CRABCACHE_LOG_FORMAT` - Log format: `json`, `pretty` (default: `json`)

## 🔌 Ports

- **8000** - Main CrabCache server
- **8001** - Metrics and monitoring (Prometheus endpoint)

## 📈 Monitoring

### Prometheus Metrics
```bash
curl http://localhost:8001/metrics
```

### Health Check
```bash
curl http://localhost:8001/health
```

### Web Dashboard
```bash
# Access the web dashboard
open http://localhost:8001/dashboard
```

## 💾 Data Persistence

### Volume Mounts

```bash
# Persistent WAL storage
docker run -p 8000:8000 \
  -v crabcache-data:/app/data/wal \
  -e CRABCACHE_ENABLE_WAL=true \
  crabcache/crabcache:latest

# Custom configuration
docker run -p 8000:8000 \
  -v /path/to/config:/app/config \
  -v /path/to/data:/app/data \
  crabcache/crabcache:latest
```

## 🧪 Testing the Container

### Basic Connectivity Test
```bash
# Start container
docker run -d -p 8000:8000 --name crabcache-test crabcache/crabcache:latest

# Test basic operations
echo "PING" | nc localhost 8000                    # Should return: PONG
echo "PUT test_key test_value" | nc localhost 8000 # Should return: OK
echo "GET test_key" | nc localhost 8000            # Should return: test_value

# Cleanup
docker stop crabcache-test && docker rm crabcache-test
```

### Pipeline Performance Test
```bash
# Test pipeline functionality (requires Python)
python3 -c "
import socket
sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.connect(('localhost', 8000))

# Send batch of commands
batch = 'PUT key1 value1\nPUT key2 value2\nGET key1\nGET key2\nPING\n'
sock.send(batch.encode())

# Read responses
for i in range(5):
    response = sock.recv(1024).decode().strip()
    print(f'Response {i+1}: {response}')

sock.close()
"
```

## 🐳 Docker Compose

```yaml
version: '3.8'

services:
  crabcache:
    image: crabcache/crabcache:latest
    ports:
      - "8000:8000"
      - "8001:8001"
    environment:
      - CRABCACHE_ENABLE_PIPELINING=true
      - CRABCACHE_MAX_BATCH_SIZE=16
      - CRABCACHE_ENABLE_WAL=true
    volumes:
      - crabcache-data:/app/data/wal
    healthcheck:
      test: ["CMD-SHELL", "echo 'PING' | nc -w 3 localhost 8000 | grep -q 'PONG'"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 20s

volumes:
  crabcache-data:
```

## 🔧 Advanced Configuration

### Custom Configuration File

Create a `config/default.toml` file:

```toml
# Server settings
bind_addr = "0.0.0.0"
port = 8000

# Pipeline configuration
[connection.pipeline]
enabled = true
max_batch_size = 32
buffer_size = 32768
timeout_ms = 100

# Security
[security]
enable_auth = true
allowed_ips = ["192.168.1.0/24", "10.0.0.0/8"]

# WAL persistence
enable_wal = true
wal_dir = "./data/wal"

[wal]
sync_policy = "async"
max_segment_size = 134217728  # 128MB
```

Mount the configuration:

```bash
docker run -p 8000:8000 \
  -v /path/to/config:/app/config \
  -v /path/to/data:/app/data \
  crabcache/crabcache:latest
```

## 📊 Performance Comparison

| Metric | CrabCache | Redis | Improvement |
|--------|-----------|-------|-------------|
| **Pipeline Ops/sec** | **219,540** | 37,498 | **5.9x FASTER** 🏆 |
| **Mixed Workload** | **205,724** | ~30,000 | **6.9x FASTER** 🏆 |
| **Average Latency** | **0.00ms** | ~0.13ms | **100x BETTER** |
| **P99 Latency** | **0.02ms** | ~0.5ms | **25x BETTER** |

## 🛡️ Security Features

- **Token-based authentication** with multiple tokens support
- **Rate limiting** using token bucket algorithm
- **IP filtering** with CIDR support (IPv4/IPv6)
- **Connection limits** and timeouts
- **Non-root container** execution
- **Health checks** for monitoring

## 🔍 Troubleshooting

### Container Won't Start
```bash
# Check logs
docker logs <container-id>

# Check if ports are available
netstat -tulpn | grep :8000
```

### Performance Issues
```bash
# Check container resources
docker stats <container-id>

# Increase memory limit
docker run --memory=4g -p 8000:8000 crabcache/crabcache:latest
```

### Connection Issues
```bash
# Test connectivity
docker exec -it <container-id> nc -z localhost 8000

# Check health status
docker inspect <container-id> | grep Health -A 10
```

## 📚 Documentation

- **GitHub Repository**: https://github.com/crabcache/crabcache
- **Performance Benchmarks**: See repository docs/
- **Configuration Guide**: See config/default.toml
- **API Documentation**: See repository README.md

## 🏷️ Tags

- `latest` - Latest stable release
- `1.0.0` - Specific version
- `1.0` - Major version
- `1` - Major version (short)

## 📄 License

MIT License - see the [LICENSE](https://github.com/crabcache/crabcache/blob/main/LICENSE) file for details.

---

**CrabCache v1.0.0** - Ultra-high-performance caching, now containerized! 🚀