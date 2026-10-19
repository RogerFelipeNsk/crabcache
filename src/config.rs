//! Command-line and environment configuration.

use crate::protocol::Limits;
use crate::store::Policy;
use crate::store::compress::CompressionAlgo;
use crate::util::parse_memory;
use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "crabcache",
    version,
    about = "Redis-compatible in-memory cache with optional CrabPack compression"
)]
pub struct Config {
    /// Address to listen on. Use 0.0.0.0 only together with --requirepass or a firewall.
    #[arg(long, env = "CRABCACHE_BIND", default_value = "127.0.0.1")]
    pub bind: String,

    #[arg(long, env = "CRABCACHE_PORT", default_value_t = 6379)]
    pub port: u16,

    /// Worker threads (0 = number of CPUs).
    #[arg(long, env = "CRABCACHE_THREADS", default_value_t = 0)]
    pub threads: usize,

    /// Keyspace shards, rounded up to a power of two (0 = 64 per worker thread).
    #[arg(long, env = "CRABCACHE_SHARDS", default_value_t = 0)]
    pub shards: usize,

    /// Memory limit for stored data, e.g. 512mb or 4gb (0 = unlimited).
    #[arg(long, env = "CRABCACHE_MAXMEMORY", default_value = "0", value_parser = parse_mem_arg)]
    pub maxmemory: u64,

    /// noeviction | allkeys-lru | allkeys-lfu | allkeys-random
    #[arg(long, env = "CRABCACHE_MAXMEMORY_POLICY", default_value = "noeviction", value_parser = parse_policy_arg)]
    pub maxmemory_policy: Policy,

    /// Candidates sampled per eviction.
    #[arg(long, env = "CRABCACHE_MAXMEMORY_SAMPLES", default_value_t = 5)]
    pub maxmemory_samples: usize,

    /// Require clients to AUTH with this password.
    #[arg(long, env = "CRABCACHE_REQUIREPASS", hide_env_values = true)]
    pub requirepass: Option<String>,

    #[arg(long, env = "CRABCACHE_MAXCLIENTS", default_value_t = 10_000)]
    pub maxclients: usize,

    /// Connections per active I/O thread before another thread is brought into use (0 = spread every
    /// connection across all threads). Lightly loaded threads waste CPU on wakeups, so connections are
    /// packed onto as few threads as their count warrants.
    #[arg(long, env = "CRABCACHE_IO_CONNS_PER_THREAD", default_value_t = 32)]
    pub io_conns_per_thread: usize,

    /// CrabPack: compress idle values with zstd dictionaries trained per key prefix.
    #[arg(long, env = "CRABCACHE_COMPRESSION")]
    pub compression: bool,

    /// auto | zstd | lz4
    #[arg(long, env = "CRABCACHE_COMPRESSION_ALGORITHM", default_value = "auto", value_parser = parse_algo_arg)]
    pub compression_algorithm: CompressionAlgo,

    /// Seconds without access before a value may be compressed.
    #[arg(long, env = "CRABCACHE_COMPRESSION_MIN_IDLE", default_value_t = 60)]
    pub compression_min_idle: u64,

    /// Values smaller than this many bytes are never compressed.
    #[arg(long, env = "CRABCACHE_COMPRESSION_MIN_SIZE", default_value_t = 64)]
    pub compression_min_size: usize,

    /// Maximum input bytes copied/compressed per 200 ms tick. Larger entries stay plain.
    #[arg(long, env = "CRABCACHE_COMPRESSION_MAX_BYTES_PER_TICK", default_value = "512kb", value_parser = parse_pack_bytes)]
    pub compression_max_bytes_per_tick: u64,

    /// Cooperative packing deadline per tick (checked between values; training is separate).
    #[arg(long, env = "CRABCACHE_COMPRESSION_TIME_BUDGET_MS", default_value_t = 2, value_parser = clap::value_parser!(u64).range(1..=1000))]
    pub compression_time_budget_ms: u64,

    /// Select dictionary size/level by payload plus prepared-table memory.
    #[arg(long, env = "CRABCACHE_COMPRESSION_ADAPTIVE_DICTS", default_value_t = true, action = clap::ArgAction::Set)]
    pub compression_adaptive_dicts: bool,

    /// Back off maintenance after sampled slow commands; does not preempt training.
    #[arg(long, env = "CRABCACHE_COMPRESSION_ADAPTIVE_MAINTENANCE", default_value_t = true, action = clap::ArgAction::Set)]
    pub compression_adaptive_maintenance: bool,

    /// Skip values with less than this many seconds of TTL left (0 disables).
    #[arg(long, env = "CRABCACHE_COMPRESSION_MIN_TTL", default_value_t = 2)]
    pub compression_min_ttl: u64,

    /// Optional resident budget for promoting hot LFU packed values to plain (0 disables).
    #[arg(long, env = "CRABCACHE_COMPRESSION_HOT_MAXMEMORY", default_value = "0", value_parser = parse_mem_arg)]
    pub compression_hot_maxmemory: u64,

    /// Target command latency in microseconds before adaptive compression backs off and prefers LZ4/plain.
    #[arg(long, env = "CRABCACHE_COMPRESSION_LATENCY_TARGET_US", default_value_t = 500, value_parser = clap::value_parser!(u64).range(1..=1_000_000))]
    pub compression_latency_target_us: u64,

    /// Largest accepted bulk string (value or key).
    #[arg(long, env = "CRABCACHE_PROTO_MAX_BULK_LEN", default_value = "512mb", value_parser = parse_mem_arg)]
    pub proto_max_bulk_len: u64,

    /// Per-client limit on buffered, not yet executed request bytes.
    #[arg(long, env = "CRABCACHE_CLIENT_QUERY_BUFFER_LIMIT", default_value = "1gb", value_parser = parse_mem_arg)]
    pub client_query_buffer_limit: u64,
}

impl Config {
    pub fn worker_threads(&self) -> usize {
        if self.threads > 0 {
            self.threads
        } else {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        }
    }

    pub fn shard_count(&self) -> usize {
        if self.shards > 0 {
            self.shards
        } else {
            64 * self.worker_threads()
        }
    }

    pub fn limits(&self) -> Limits {
        Limits {
            max_bulk_len: self.proto_max_bulk_len as usize,
            ..Limits::default()
        }
    }

    /// Defaults suitable for tests: ephemeral port on localhost, few threads.
    pub fn for_tests() -> Self {
        Config::parse_from(["crabcache", "--port", "0", "--threads", "2"])
    }
}

fn parse_mem_arg(s: &str) -> Result<u64, String> {
    parse_memory(s.as_bytes()).ok_or_else(|| format!("invalid memory size '{s}'"))
}

fn parse_policy_arg(s: &str) -> Result<Policy, String> {
    Policy::parse(s.as_bytes()).ok_or_else(|| format!("unknown eviction policy '{s}'"))
}

fn parse_algo_arg(s: &str) -> Result<CompressionAlgo, String> {
    CompressionAlgo::parse(s.as_bytes())
        .ok_or_else(|| format!("unknown compression algorithm '{s}'"))
}

fn parse_pack_bytes(s: &str) -> Result<u64, String> {
    let bytes = parse_mem_arg(s)?;
    if bytes == 0 {
        Err("compression byte budget must be positive".into())
    } else {
        Ok(bytes)
    }
}
