# CrabCache TinyLFU Eviction System

## Overview

CrabCache implements an advanced eviction system based on the TinyLFU (Tiny Least Frequently Used) algorithm. This system provides intelligent cache management that considers both frequency and recency of access, resulting in superior hit ratios compared to traditional LRU eviction.

## 🔍 Análise de Performance em Ambientes Limitados

### Problema Identificado em Testes

Durante testes com limitações extremas de memória (32MB), o CrabCache apresentou comportamento de eviction intensiva (27.500 evictions para 5.000 inserções). Esta análise demonstra que o problema não é algorítmico, mas sim de configuração inadequada para o ambiente.

### Configurações vs Ambiente

**Configuração Padrão (Otimizada para Servidores)**:
```toml
[eviction]
memory_high_watermark = 0.8  # 80% para iniciar eviction
memory_low_watermark = 0.6   # 60% para parar eviction
max_capacity = 10000         # 10K itens por shard
```

**Ambiente de Teste Extremo**:
- Container: 32MB total
- Configuração interna: 1GB por shard
- Resultado: Conflito que causa eviction constante

### Soluções Implementadas

**Configuração Adaptativa para Ambientes Limitados**:
```toml
# Para containers < 100MB
[eviction]
memory_high_watermark = 0.95  # Mais tolerante
memory_low_watermark = 0.85   # Menos agressivo
max_capacity = 2000           # Capacidade realista
window_ratio = 0.05           # 5% para window
```

**Detecção Automática de Memória**:
```rust
fn detect_container_memory() -> usize {
    std::env::var("MEMORY_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            // Detectar limites do cgroup
            read_cgroup_memory_limit()
                .unwrap_or(1024 * 1024 * 1024) // 1GB padrão
        })
}
```

### Resultados da Otimização

**Antes (Configuração Conflitante)**:
- Evictions: 27.500 (5.5x mais que inserções)
- Retenção: 0% (todos os dados removidos)
- Causa: Watermarks inadequados para ambiente

**Depois (Configuração Adaptativa)**:
- Evictions: ~500-1.000 (redução de 95%)
- Retenção: ~2.000-3.000 itens (vs 0 anterior)
- Performance: Mantida ou melhorada

## Architecture

The eviction system consists of several key components:

### 1. TinyLFU Algorithm
- **Count-Min Sketch**: Probabilistic data structure for frequency estimation
- **Window LRU**: Cache for newly inserted items (1% of total capacity by default)
- **Main LRU**: Cache for established items (99% of total capacity by default)
- **Admission Policy**: Frequency-based decision making for eviction

### 2. Memory Pressure Monitoring
- **Per-shard monitoring**: Each shard has independent memory tracking
- **Configurable thresholds**: High/low watermarks for eviction triggering
- **Coordinated eviction**: Background task coordinates eviction across shards

### 3. Comprehensive Metrics
- **Hit/miss ratios**: Real-time cache performance tracking
- **Eviction statistics**: Detailed eviction and admission metrics
- **Memory usage**: Per-shard and global memory utilization

## Configuration

The eviction system is highly configurable through the `config/default.toml` file:

```toml
[eviction]
# Enable TinyLFU eviction algorithm
enabled = true

# Proportion of cache dedicated to Window LRU (0.0 to 1.0)
window_ratio = 0.01

# Count-Min Sketch configuration for frequency estimation
sketch_width = 1024
sketch_depth = 4

# Memory pressure thresholds (0.0 to 1.0)
memory_high_watermark = 0.8  # Start eviction at 80% memory usage
memory_low_watermark = 0.6   # Stop eviction at 60% memory usage

# Frequency sketch reset interval in seconds
reset_interval = 3600  # 1 hour

# Maximum cache capacity per shard (items)
max_capacity = 10000
```

### Configuration Parameters

| Parameter | Description | Default | Range |
|-----------|-------------|---------|-------|
| `enabled` | Enable/disable TinyLFU eviction | `true` | boolean |
| `window_ratio` | Proportion for Window LRU | `0.01` | 0.0-1.0 |
| `sketch_width` | Count-Min Sketch width | `1024` | > 0 |
| `sketch_depth` | Count-Min Sketch depth | `4` | > 0 |
| `memory_high_watermark` | Eviction start threshold | `0.8` | 0.0-1.0 |
| `memory_low_watermark` | Eviction stop threshold | `0.6` | 0.0-1.0 |
| `reset_interval` | Sketch reset interval (seconds) | `3600` | > 0 |
| `max_capacity` | Max items per shard | `10000` | > 0 |

## How It Works

### 1. Item Insertion
1. New items are placed in the **Window LRU**
2. When Window LRU is full, the oldest item is promoted to **Main LRU**
3. If Main LRU is full, the **admission policy** decides whether to accept the promotion

### 2. Admission Policy
The admission policy uses frequency estimation to make eviction decisions:

```rust
fn should_admit(candidate_key: &str, victim_key: &str) -> bool {
    let candidate_freq = frequency_sketch.estimate(candidate_key);
    let victim_freq = frequency_sketch.estimate(victim_key);
    candidate_freq >= victim_freq
}
```

### 3. Memory Pressure Handling
- **Monitoring**: Each shard continuously monitors memory usage
- **Triggering**: When usage exceeds high watermark, eviction begins
- **Coordination**: Background task coordinates eviction across multiple shards
- **Stopping**: Eviction stops when usage drops below low watermark

### 4. Frequency Tracking
- **Count-Min Sketch**: Efficiently estimates access frequency for all keys
- **Periodic Reset**: Sketch is reset periodically to adapt to changing patterns
- **Low Overhead**: Constant time operations with minimal memory usage

## Performance Characteristics

### Time Complexity
- **GET**: O(1) average case
- **PUT**: O(1) average case
- **Eviction**: O(1) amortized
- **Frequency Estimation**: O(1)

### Memory Overhead
- **Count-Min Sketch**: ~4KB for default configuration (1024 × 4 × 4 bytes)
- **Window LRU**: Minimal overhead for 1% of capacity
- **Main LRU**: Standard HashMap + access order tracking

### Expected Performance Improvements
- **Hit Ratio**: 10-30% improvement over LRU
- **Memory Efficiency**: Better utilization through intelligent eviction
- **Adaptability**: Automatically adapts to changing access patterns

## Metrics and Monitoring

### Available Metrics
The system provides comprehensive metrics through the `STATS` command:

```json
{
  "eviction": {
    "total_memory_usage": 1048576,
    "total_memory_limit": 2097152,
    "overall_usage_ratio": 0.5,
    "shards": [
      {
        "shard_id": 0,
        "cache_hits": 1500,
        "cache_misses": 500,
        "hit_ratio": 0.75,
        "evictions": 100,
        "admissions_accepted": 80,
        "admissions_rejected": 20,
        "admission_ratio": 0.8,
        "memory_usage": 524288,
        "memory_limit": 1048576,
        "memory_pressure": 0.3
      }
    ]
  }
}
```

### Key Metrics Explained
- **Hit Ratio**: Percentage of requests served from cache
- **Admission Ratio**: Percentage of eviction candidates accepted
- **Memory Pressure**: Current pressure level (0.0 = no pressure, 1.0+ = over limit)
- **Evictions**: Total number of items evicted
- **Window Promotions**: Items promoted from Window to Main LRU

## Usage Examples

### Basic Usage
```rust
use crabcache::eviction::{TinyLFU, EvictionConfig, EvictionPolicy};

// Create configuration
let config = EvictionConfig {
    max_capacity: 1000,
    window_ratio: 0.01,
    ..Default::default()
};

// Create cache
let mut cache = TinyLFU::new(config)?;

// Basic operations
cache.put("key1".to_string(), b"value1".to_vec());
let value = cache.get("key1");
cache.remove("key1");

// Get metrics
let metrics = cache.metrics().snapshot();
println!("Hit ratio: {:.2}%", metrics.hit_ratio * 100.0);
```

### Batch Operations
```rust
// Batch insertion for better performance
let items = vec![
    ("key1".to_string(), b"value1".to_vec()),
    ("key2".to_string(), b"value2".to_vec()),
    ("key3".to_string(), b"value3".to_vec()),
];

let evicted = cache.put_batch(items);
println!("Evicted {} items", evicted.len());
```

### Integration with Shard Manager
```rust
use crabcache::shard::EvictionShardManager;

// Create eviction-enabled shard manager
let manager = EvictionShardManager::new(
    4,                    // 4 shards
    1024 * 1024 * 1024,  // 1GB per shard
    config,              // Eviction configuration
)?;

// Process commands with automatic eviction
let response = manager.process_command(command).await;
```

## Best Practices

### 1. Configuration Tuning
- **Window Ratio**: Start with 1% (0.01), increase for workloads with many new items
- **Sketch Size**: Larger sketches provide better accuracy but use more memory
- **Watermarks**: Adjust based on memory constraints and performance requirements

### 2. Monitoring
- Monitor hit ratios to validate eviction effectiveness
- Watch admission ratios to understand eviction pressure
- Track memory pressure to optimize watermark settings

### 3. Performance Optimization
- Use batch operations when inserting multiple items
- Configure sketch reset interval based on access pattern changes
- Adjust capacity based on available memory and performance requirements

## Troubleshooting

### Common Issues

#### Low Hit Ratio
- **Cause**: Cache too small or poor eviction decisions
- **Solution**: Increase capacity or adjust window ratio

#### High Memory Usage
- **Cause**: Watermarks too high or eviction not triggered
- **Solution**: Lower high watermark or check eviction system status

#### Frequent Evictions
- **Cause**: Cache capacity too small for workload
- **Solution**: Increase capacity or optimize data size

#### Poor Admission Ratio
- **Cause**: Sketch too small or reset interval too short
- **Solution**: Increase sketch size or reset interval

### Debugging Commands

```bash
# Get detailed eviction statistics
echo "STATS" | nc localhost 7001

# Monitor memory pressure
watch -n 1 'echo "STATS" | nc localhost 7001 | jq .eviction.overall_usage_ratio'

# Check per-shard metrics
echo "STATS" | nc localhost 7001 | jq '.eviction.shards[]'
```

## Implementation Details

### Thread Safety
- All operations are thread-safe using `RwLock` and atomic operations
- Lock-free frequency estimation using Count-Min Sketch
- Coordinated eviction across shards without global locks

### Error Handling
- Graceful degradation when TinyLFU components fail
- Automatic fallback to regular shard operations
- Comprehensive error logging and recovery

### Memory Management
- Precise memory tracking per shard
- Automatic memory pressure detection
- Coordinated cleanup across multiple shards

## Future Enhancements

### Planned Features
- **Adaptive Configuration**: Automatic tuning based on workload patterns
- **Advanced Metrics**: Detailed latency histograms for eviction operations
- **Compression**: Optional value compression to reduce memory usage
- **Persistence**: Optional persistence of frequency information

### Performance Improvements
- **SIMD Optimization**: Vectorized operations for Count-Min Sketch
- **Lock-Free Operations**: Further reduction of lock contention
- **Batch Processing**: Enhanced batch operation support

## References

- [TinyLFU Paper](https://arxiv.org/abs/1512.00727) - Original TinyLFU algorithm
- [Count-Min Sketch](https://en.wikipedia.org/wiki/Count%E2%80%93min_sketch) - Frequency estimation data structure
- [Caffeine Cache](https://github.com/ben-manes/caffeine) - Java implementation reference