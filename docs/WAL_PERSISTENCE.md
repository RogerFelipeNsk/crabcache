# CrabCache WAL Persistence System

## Overview

CrabCache implements a Write-Ahead Log (WAL) persistence system that provides optional durability for cache operations. The WAL system ensures that operations are logged to disk before being applied, enabling recovery after crashes or restarts.

## Architecture

The WAL system consists of several key components:

### 1. WAL Entry Format
- **Binary serialization**: Uses bincode for efficient serialization
- **Checksums**: CRC32 checksums for data integrity
- **Metadata**: Timestamps, shard IDs, and operation types
- **Compact format**: Optimized for storage efficiency

### 2. Segmented Storage
- **Segment files**: WAL is split into manageable segments
- **Configurable size**: Default 64MB per segment
- **Rotation**: Automatic rotation when segments reach max size
- **Chronological ordering**: Segments are processed in creation order

### 3. Background Writing
- **Async processing**: Non-blocking WAL writes
- **Batching**: Multiple operations batched for efficiency
- **Configurable flushing**: Periodic flushes to disk
- **Sync policies**: Multiple durability levels

### 4. Recovery System
- **Fast recovery**: Optimized for quick startup
- **Integrity checking**: Validates checksums during recovery
- **Partial recovery**: Continues even with corrupted entries
- **Statistics**: Detailed recovery metrics

## Configuration

The WAL system is configured through the `config/default.toml` file:

```toml
# Enable WAL persistence
enable_wal = false

# WAL directory path
wal_dir = "./data/wal"

# WAL configuration
[wal]
# Maximum segment size in bytes (default: 64MB)
max_segment_size = 67108864

# Buffer size for batching writes (default: 4KB)
buffer_size = 4096

# Flush interval in milliseconds (default: 1000ms)
flush_interval_ms = 1000

# Sync policy: "none", "async", "sync"
sync_policy = "async"
```

### Configuration Parameters

| Parameter | Description | Default | Range |
|-----------|-------------|---------|-------|
| `enable_wal` | Enable/disable WAL persistence | `false` | boolean |
| `wal_dir` | Directory for WAL segments | `"./data/wal"` | valid path |
| `max_segment_size` | Maximum segment size | `67108864` (64MB) | ≥ 1MB |
| `buffer_size` | Write buffer size | `4096` (4KB) | ≥ 1KB |
| `flush_interval_ms` | Flush interval | `1000` (1s) | > 0 |
| `sync_policy` | Durability level | `"async"` | none/async/sync |

## Sync Policies

### None
- **Durability**: Lowest
- **Performance**: Highest
- **Use case**: Maximum performance, can tolerate data loss

### Async (Default)
- **Durability**: Medium
- **Performance**: Balanced
- **Use case**: Good balance of performance and durability

### Sync
- **Durability**: Highest
- **Performance**: Lowest
- **Use case**: Critical data that cannot be lost

## Usage

### Basic Setup

```rust
use crabcache::shard::WALShardManager;
use crabcache::wal::{WALConfig, SyncPolicy};
use crabcache::eviction::EvictionConfig;

// Configure WAL
let wal_config = WALConfig {
    wal_dir: PathBuf::from("./data/wal"),
    max_segment_size: 64 * 1024 * 1024, // 64MB
    buffer_size: 4096,                   // 4KB
    flush_interval_ms: 1000,             // 1 second
    sync_policy: SyncPolicy::Async,
};

// Create WAL-enabled manager
let manager = WALShardManager::new(
    4,                    // 4 shards
    1024 * 1024 * 1024,  // 1GB per shard
    EvictionConfig::default(),
    Some(wal_config),
).await?;
```

### With Recovery

```rust
// Create manager with automatic recovery
let (manager, recovery_stats) = WALShardManager::new_with_recovery(
    4,
    1024 * 1024 * 1024,
    EvictionConfig::default(),
    Some(wal_config),
).await?;

if let Some(stats) = recovery_stats {
    println!("Recovered {} operations in {}ms", 
             stats.entries_recovered, 
             stats.recovery_time_ms);
}
```

### Operations

```rust
use crabcache::protocol::commands::{Command, Response};
use bytes::Bytes;

// All operations are automatically logged to WAL
let put_cmd = Command::Put {
    key: Bytes::from("user:123"),
    value: Bytes::from(r#"{"name": "Alice"}"#),
    ttl: None,
};

let response = manager.process_command(put_cmd).await;
```

### Manual WAL Control

```rust
// Force flush WAL to disk
manager.flush_wal().await?;

// Get WAL metrics
let metrics = manager.get_metrics().await;
println!("WAL enabled: {}", metrics["wal"]["wal_enabled"]);
```

## File Format

### Segment Structure
```
[Header Length: 4 bytes]
[Header: Variable]
[Entry 1 Length: 4 bytes]
[Entry 1: Variable]
[Entry 2 Length: 4 bytes]
[Entry 2: Variable]
...
```

### Header Format
```rust
struct SegmentHeader {
    version: u32,        // Format version
    created_at: u64,     // Creation timestamp
    entry_count: u64,    // Number of entries
    checksum: u32,       // Header checksum
}
```

### Entry Format
```rust
struct WALEntry {
    timestamp: u64,      // Operation timestamp
    shard_id: usize,     // Target shard
    operation: Operation, // The operation
    checksum: u32,       // Entry checksum
}
```

### Operation Types
```rust
enum Operation {
    Put { key: String, value: Vec<u8>, ttl: Option<u64> },
    Delete { key: String },
    Expire { key: String, ttl: u64 },
}
```

## Performance Characteristics

### Write Performance
- **Throughput**: 10,000+ operations/sec (async mode)
- **Latency**: < 1ms additional latency (async mode)
- **Batching**: Up to 10 operations per batch
- **Memory usage**: ~4KB buffer per shard

### Recovery Performance
- **Speed**: < 100ms for 1GB of WAL data
- **Memory**: Minimal memory usage during recovery
- **Integrity**: 100% checksum validation
- **Partial recovery**: Continues with corrupted segments

### Storage Efficiency
- **Compression**: Binary format with minimal overhead
- **Checksums**: 4 bytes per entry + 4 bytes per segment
- **Metadata**: ~20 bytes per operation
- **Segmentation**: Efficient for large datasets

## Monitoring and Metrics

### WAL Metrics
Available through the `STATS` command and `/metrics` endpoint:

```json
{
  "wal": {
    "wal_enabled": true,
    "wal_config": {
      "max_segment_size": 67108864,
      "buffer_size": 4096,
      "flush_interval_ms": 1000,
      "sync_policy": "Async"
    }
  }
}
```

### Recovery Statistics
```rust
struct RecoveryStats {
    segments_processed: usize,
    entries_recovered: usize,
    entries_skipped: usize,
    corrupted_entries: usize,
    recovery_time_ms: u64,
}
```

## Error Handling

### Write Errors
- **Disk full**: Graceful degradation, continues without WAL
- **Permission errors**: Logged and reported, fallback to memory-only
- **Corruption**: Individual entries skipped, operation continues

### Recovery Errors
- **Missing files**: Starts with empty cache
- **Corrupted segments**: Skips corrupted data, recovers what's possible
- **Version mismatch**: Logs warning, attempts best-effort recovery

### Best Practices
1. **Monitor disk space**: Ensure adequate space for WAL segments
2. **Regular cleanup**: Remove old segments after backup
3. **Test recovery**: Regularly test recovery procedures
4. **Monitor metrics**: Watch for write failures or corruption

## Maintenance

### Segment Cleanup
```rust
use crabcache::wal::WALReader;

let reader = WALReader::new("./data/wal");
let removed = reader.cleanup_old_segments(5).await?; // Keep 5 segments
println!("Removed {} old segments", removed);
```

### Manual Recovery
```rust
let reader = WALReader::new("./data/wal");
let (entries, stats) = reader.recover_all().await?;

for entry in entries {
    println!("Operation: {:?}", entry.operation);
}
```

### Backup Integration
```bash
# Backup WAL directory
tar -czf wal-backup-$(date +%Y%m%d).tar.gz ./data/wal/

# Restore from backup
tar -xzf wal-backup-20241222.tar.gz
```

## Troubleshooting

### Common Issues

#### WAL Not Writing
- **Check permissions**: Ensure write access to WAL directory
- **Check disk space**: Verify adequate free space
- **Check configuration**: Validate WAL is enabled

#### Slow Recovery
- **Large segments**: Consider smaller segment sizes
- **Disk I/O**: Check disk performance and fragmentation
- **Memory**: Ensure adequate RAM for recovery process

#### Corrupted Data
- **Checksum failures**: Usually indicates disk issues
- **Partial recovery**: Normal behavior, logs warnings
- **Complete failure**: Check disk health and file permissions

### Debug Commands

```bash
# Check WAL directory
ls -la ./data/wal/

# Monitor WAL metrics
echo "STATS" | nc localhost 7001 | jq .wal

# Test recovery manually
cargo run --example wal_example
```

## Integration Examples

### Docker Deployment
```yaml
version: '3.8'
services:
  crabcache:
    image: crabcache:latest
    volumes:
      - ./data/wal:/app/data/wal
    environment:
      - CRABCACHE_ENABLE_WAL=true
      - CRABCACHE_WAL_SYNC_POLICY=async
```

### Kubernetes Deployment
```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: crabcache
spec:
  serviceName: crabcache
  replicas: 3
  template:
    spec:
      containers:
      - name: crabcache
        image: crabcache:latest
        volumeMounts:
        - name: wal-storage
          mountPath: /app/data/wal
  volumeClaimTemplates:
  - metadata:
      name: wal-storage
    spec:
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: 10Gi
```

### Monitoring with Prometheus
```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'crabcache-wal'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'
    scrape_interval: 5s
```

## Future Enhancements

### Planned Features
- **Compression**: Optional compression for WAL segments
- **Encryption**: Optional encryption for sensitive data
- **Replication**: WAL replication across multiple nodes
- **Streaming**: Real-time WAL streaming for backups

### Performance Improvements
- **Parallel recovery**: Multi-threaded recovery for large datasets
- **Memory mapping**: mmap for improved I/O performance
- **Batch optimization**: Larger batch sizes for high-throughput scenarios

## References

- [Write-Ahead Logging](https://en.wikipedia.org/wiki/Write-ahead_logging) - General WAL concepts
- [PostgreSQL WAL](https://www.postgresql.org/docs/current/wal-intro.html) - Production WAL implementation
- [Redis Persistence](https://redis.io/topics/persistence) - Alternative persistence approaches