//! Configuration management for CrabCache

use crate::eviction::EvictionConfig;
use crate::wal::{SyncPolicy, WALConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// CrabCache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server bind address
    pub bind_addr: String,
    /// Server port
    pub port: u16,
    /// Number of shards (defaults to CPU count)
    pub num_shards: Option<usize>,
    /// Maximum memory per shard in bytes
    pub max_memory_per_shard: usize,
    /// Enable WAL persistence
    pub enable_wal: bool,
    /// WAL directory path
    pub wal_dir: String,
    /// WAL configuration
    pub wal: WALConfigToml,
    /// Metrics export settings
    pub metrics: MetricsConfig,
    /// Eviction policy settings
    pub eviction: EvictionConfig,
    /// Security settings
    pub security: SecurityConfig,
    /// Rate limiting settings
    pub rate_limiting: RateLimitConfig,
    /// Connection settings
    pub connection: ConnectionConfig,
    /// Logging settings
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable Prometheus metrics endpoint
    pub enabled: bool,
    /// Metrics endpoint path
    pub path: String,
    /// Metrics server port
    pub port: u16,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable authentication
    pub enable_auth: bool,
    /// Authentication token (if auth enabled)
    pub auth_token: Option<String>,
    /// Allowed client IPs (empty = allow all)
    pub allowed_ips: Vec<String>,
    /// Enable TLS
    pub enable_tls: bool,
    /// TLS certificate path
    pub tls_cert_path: Option<String>,
    /// TLS private key path
    pub tls_key_path: Option<String>,
    /// Maximum command size in bytes
    pub max_command_size: usize,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,
    /// Maximum requests per second per client
    pub max_requests_per_second: u32,
    /// Burst capacity
    pub burst_capacity: u32,
    /// Rate limit window in seconds
    pub window_seconds: u32,
}

/// Connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Connection timeout in seconds
    pub connection_timeout_seconds: u64,
    /// Keep-alive timeout in seconds
    pub keepalive_timeout_seconds: u64,
    /// TCP nodelay (disable Nagle's algorithm)
    pub tcp_nodelay: bool,
    /// TCP buffer sizes
    pub tcp_recv_buffer_size: Option<usize>,
    pub tcp_send_buffer_size: Option<usize>,
    /// Pipeline configuration
    pub pipeline: PipelineConfig,
}

/// Pipeline configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Enable pipelining
    pub enabled: bool,
    /// Maximum batch size for pipeline processing
    pub max_batch_size: usize,
    /// Pipeline buffer size
    pub buffer_size: usize,
    /// Pipeline timeout in milliseconds
    pub timeout_ms: u64,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,
    /// Log format (json, pretty)
    pub format: String,
    /// Enable file logging
    pub enable_file: bool,
    /// Log file path
    pub file_path: Option<String>,
    /// Maximum log file size in MB
    pub max_file_size_mb: u64,
    /// Number of log files to keep
    pub max_files: u32,
}

/// WAL configuration from TOML (with string sync_policy)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WALConfigToml {
    /// Maximum segment size in bytes
    pub max_segment_size: u64,
    /// Buffer size for batching writes
    pub buffer_size: usize,
    /// Flush interval in milliseconds
    pub flush_interval_ms: u64,
    /// Sync policy as string
    pub sync_policy: String,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_auth: false,
            auth_token: None,
            allowed_ips: vec![],
            enable_tls: false,
            tls_cert_path: None,
            tls_key_path: None,
            max_command_size: 1024 * 1024, // 1MB
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_requests_per_second: 1000,
            burst_capacity: 100,
            window_seconds: 1,
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_batch_size: 16,
            buffer_size: 16384, // 16KB
            timeout_ms: 100,
        }
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_connections: 1000,
            connection_timeout_seconds: 30,
            keepalive_timeout_seconds: 300,
            tcp_nodelay: true,
            tcp_recv_buffer_size: Some(65536), // 64KB
            tcp_send_buffer_size: Some(65536), // 64KB
            pipeline: PipelineConfig::default(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "json".to_string(),
            enable_file: false,
            file_path: Some("./logs/crabcache.log".to_string()),
            max_file_size_mb: 100,
            max_files: 10,
        }
    }
}

impl Default for WALConfigToml {
    fn default() -> Self {
        Self {
            max_segment_size: 64 * 1024 * 1024, // 64MB
            buffer_size: 4096,                  // 4KB
            flush_interval_ms: 1000,            // 1 second
            sync_policy: "async".to_string(),
        }
    }
}

impl WALConfigToml {
    /// Convert to WALConfig with parsed sync policy
    pub fn to_wal_config(&self, wal_dir: PathBuf) -> Result<WALConfig, String> {
        let sync_policy = match self.sync_policy.as_str() {
            "none" => SyncPolicy::None,
            "async" => SyncPolicy::Async,
            "sync" => SyncPolicy::Sync,
            _ => return Err(format!("Invalid sync policy: {}", self.sync_policy)),
        };

        Ok(WALConfig {
            wal_dir,
            max_segment_size: self.max_segment_size,
            buffer_size: self.buffer_size,
            flush_interval_ms: self.flush_interval_ms,
            sync_policy,
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1".to_string(),
            port: 7000,
            num_shards: None,                         // Will default to CPU count
            max_memory_per_shard: 1024 * 1024 * 1024, // 1GB
            enable_wal: false,
            wal_dir: "./data/wal".to_string(),
            wal: WALConfigToml::default(),
            metrics: MetricsConfig {
                enabled: true,
                path: "/metrics".to_string(),
                port: 7001,
            },
            eviction: EvictionConfig::default(),
            security: SecurityConfig::default(),
            rate_limiting: RateLimitConfig::default(),
            connection: ConnectionConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from file or use defaults
    pub async fn load() -> crate::Result<Self> {
        // Try to load from config file, fallback to defaults
        let mut config = if Path::new("config/default.toml").exists() {
            let content = fs::read_to_string("config/default.toml").await?;
            toml::from_str(&content)?
        } else {
            Config::default()
        };

        // Override with environment variables
        config.apply_env_overrides();

        // Validate all configurations
        if let Err(e) = config.eviction.validate() {
            eprintln!("Invalid eviction configuration: {}", e);
            config.eviction = EvictionConfig::default();
        }

        if let Err(e) = config.validate_wal_config() {
            eprintln!("Invalid WAL configuration: {}", e);
            config.wal = WALConfigToml::default();
        }

        if let Err(e) = config.validate_security_config() {
            eprintln!("Invalid security configuration: {}", e);
            config.security = SecurityConfig::default();
        }

        if let Err(e) = config.validate_rate_limit_config() {
            eprintln!("Invalid rate limiting configuration: {}", e);
            config.rate_limiting = RateLimitConfig::default();
        }

        if let Err(e) = config.validate_connection_config() {
            eprintln!("Invalid connection configuration: {}", e);
            config.connection = ConnectionConfig::default();
        }

        if let Err(e) = config.validate_logging_config() {
            eprintln!("Invalid logging configuration: {}", e);
            config.logging = LoggingConfig::default();
        }

        Ok(config)
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        // WAL configuration
        if let Ok(enable_wal) = std::env::var("CRABCACHE_ENABLE_WAL") {
            if let Ok(enabled) = enable_wal.parse::<bool>() {
                self.enable_wal = enabled;
                println!("WAL enabled from environment: {}", enabled);
            }
        }

        if let Ok(wal_dir) = std::env::var("CRABCACHE_WAL_DIR") {
            self.wal_dir = wal_dir;
            println!("WAL directory from environment: {}", self.wal_dir);
        }

        if let Ok(sync_policy) = std::env::var("CRABCACHE_WAL_SYNC_POLICY") {
            self.wal.sync_policy = sync_policy;
            println!("WAL sync policy from environment: {}", self.wal.sync_policy);
        }

        // Server configuration
        if let Ok(port) = std::env::var("CRABCACHE_PORT") {
            if let Ok(port_num) = port.parse::<u16>() {
                self.port = port_num;
                println!("Port from environment: {}", port_num);
            }
        }

        if let Ok(bind_addr) = std::env::var("CRABCACHE_BIND_ADDR") {
            self.bind_addr = bind_addr;
            println!("Bind address from environment: {}", self.bind_addr);
        }

        // Security configuration
        if let Ok(enable_auth) = std::env::var("CRABCACHE_ENABLE_AUTH") {
            if let Ok(enabled) = enable_auth.parse::<bool>() {
                self.security.enable_auth = enabled;
                println!("Authentication enabled from environment: {}", enabled);
            }
        }

        if let Ok(auth_token) = std::env::var("CRABCACHE_AUTH_TOKEN") {
            self.security.auth_token = Some(auth_token);
            println!("Authentication token set from environment");
        }

        if let Ok(allowed_ips) = std::env::var("CRABCACHE_ALLOWED_IPS") {
            self.security.allowed_ips = allowed_ips
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            println!(
                "Allowed IPs from environment: {:?}",
                self.security.allowed_ips
            );
        }

        if let Ok(max_command_size) = std::env::var("CRABCACHE_MAX_COMMAND_SIZE") {
            if let Ok(size) = max_command_size.parse::<usize>() {
                self.security.max_command_size = size;
                println!("Max command size from environment: {}", size);
            }
        }

        // Rate limiting configuration
        if let Ok(enable_rate_limit) = std::env::var("CRABCACHE_ENABLE_RATE_LIMIT") {
            if let Ok(enabled) = enable_rate_limit.parse::<bool>() {
                self.rate_limiting.enabled = enabled;
                println!("Rate limiting enabled from environment: {}", enabled);
            }
        }

        if let Ok(max_rps) = std::env::var("CRABCACHE_MAX_REQUESTS_PER_SECOND") {
            if let Ok(rps) = max_rps.parse::<u32>() {
                self.rate_limiting.max_requests_per_second = rps;
                println!("Max requests per second from environment: {}", rps);
            }
        }

        // Connection configuration
        if let Ok(max_connections) = std::env::var("CRABCACHE_MAX_CONNECTIONS") {
            if let Ok(max_conn) = max_connections.parse::<usize>() {
                self.connection.max_connections = max_conn;
                println!("Max connections from environment: {}", max_conn);
            }
        }

        if let Ok(timeout) = std::env::var("CRABCACHE_CONNECTION_TIMEOUT") {
            if let Ok(timeout_secs) = timeout.parse::<u64>() {
                self.connection.connection_timeout_seconds = timeout_secs;
                println!("Connection timeout from environment: {}s", timeout_secs);
            }
        }

        // Pipeline configuration
        if let Ok(enable_pipelining) = std::env::var("CRABCACHE_ENABLE_PIPELINING") {
            if let Ok(enabled) = enable_pipelining.parse::<bool>() {
                self.connection.pipeline.enabled = enabled;
                println!("Pipelining enabled from environment: {}", enabled);
            }
        }

        if let Ok(max_batch_size) = std::env::var("CRABCACHE_MAX_BATCH_SIZE") {
            if let Ok(batch_size) = max_batch_size.parse::<usize>() {
                self.connection.pipeline.max_batch_size = batch_size;
                println!("Pipeline max batch size from environment: {}", batch_size);
            }
        }

        if let Ok(pipeline_buffer_size) = std::env::var("CRABCACHE_PIPELINE_BUFFER_SIZE") {
            if let Ok(buffer_size) = pipeline_buffer_size.parse::<usize>() {
                self.connection.pipeline.buffer_size = buffer_size;
                println!("Pipeline buffer size from environment: {}", buffer_size);
            }
        }

        // Logging configuration
        if let Ok(log_level) = std::env::var("CRABCACHE_LOG_LEVEL") {
            self.logging.level = log_level;
            println!("Log level from environment: {}", self.logging.level);
        }

        if let Ok(log_format) = std::env::var("CRABCACHE_LOG_FORMAT") {
            self.logging.format = log_format;
            println!("Log format from environment: {}", self.logging.format);
        }

        // Memory and shard configuration
        if let Ok(num_shards) = std::env::var("CRABCACHE_NUM_SHARDS") {
            if let Ok(shards) = num_shards.parse::<usize>() {
                self.num_shards = Some(shards);
                println!("Number of shards from environment: {}", shards);
            }
        }

        if let Ok(max_memory) = std::env::var("CRABCACHE_MAX_MEMORY_PER_SHARD") {
            if let Ok(memory) = max_memory.parse::<usize>() {
                self.max_memory_per_shard = memory;
                println!("Max memory per shard from environment: {}", memory);
            }
        }

        // Eviction configuration
        if let Ok(enabled) = std::env::var("CRABCACHE_EVICTION_ENABLED") {
            if let Ok(eviction_enabled) = enabled.parse::<bool>() {
                self.eviction.enabled = eviction_enabled;
                println!("Eviction enabled from environment: {}", eviction_enabled);
            }
        }

        if let Ok(max_capacity) = std::env::var("CRABCACHE_EVICTION_MAX_CAPACITY") {
            if let Ok(capacity) = max_capacity.parse::<usize>() {
                self.eviction.max_capacity = capacity;
                println!("Eviction max capacity from environment: {}", capacity);
            }
        }

        if let Ok(window_ratio) = std::env::var("CRABCACHE_EVICTION_WINDOW_RATIO") {
            if let Ok(ratio) = window_ratio.parse::<f64>() {
                self.eviction.window_ratio = ratio;
                println!("Eviction window ratio from environment: {}", ratio);
            }
        }

        if let Ok(high_watermark) = std::env::var("CRABCACHE_EVICTION_HIGH_WATERMARK") {
            if let Ok(watermark) = high_watermark.parse::<f64>() {
                self.eviction.memory_high_watermark = watermark;
                println!("Eviction high watermark from environment: {}", watermark);
            }
        }

        if let Ok(low_watermark) = std::env::var("CRABCACHE_EVICTION_LOW_WATERMARK") {
            if let Ok(watermark) = low_watermark.parse::<f64>() {
                self.eviction.memory_low_watermark = watermark;
                println!("Eviction low watermark from environment: {}", watermark);
            }
        }

        if let Ok(sketch_width) = std::env::var("CRABCACHE_EVICTION_SKETCH_WIDTH") {
            if let Ok(width) = sketch_width.parse::<usize>() {
                self.eviction.sketch_width = width;
                println!("Eviction sketch width from environment: {}", width);
            }
        }

        if let Ok(sketch_depth) = std::env::var("CRABCACHE_EVICTION_SKETCH_DEPTH") {
            if let Ok(depth) = sketch_depth.parse::<usize>() {
                self.eviction.sketch_depth = depth;
                println!("Eviction sketch depth from environment: {}", depth);
            }
        }

        if let Ok(reset_interval) = std::env::var("CRABCACHE_EVICTION_RESET_INTERVAL") {
            if let Ok(interval) = reset_interval.parse::<u64>() {
                self.eviction.reset_interval_secs = interval;
                println!("Eviction reset interval from environment: {}s", interval);
            }
        }

        // New eviction strategy configurations
        if let Ok(strategy) = std::env::var("CRABCACHE_EVICTION_STRATEGY") {
            if ["batch", "gradual"].contains(&strategy.as_str()) {
                self.eviction.eviction_strategy = strategy.clone();
                println!("Eviction strategy from environment: {}", strategy);
            }
        }

        if let Ok(batch_size) = std::env::var("CRABCACHE_EVICTION_BATCH_SIZE") {
            if let Ok(size) = batch_size.parse::<usize>() {
                self.eviction.batch_eviction_size = size;
                println!("Eviction batch size from environment: {}", size);
            }
        }

        if let Ok(min_threshold) = std::env::var("CRABCACHE_EVICTION_MIN_ITEMS") {
            if let Ok(threshold) = min_threshold.parse::<usize>() {
                self.eviction.min_items_threshold = threshold;
                println!(
                    "Eviction min items threshold from environment: {}",
                    threshold
                );
            }
        }

        if let Ok(multiplier) = std::env::var("CRABCACHE_EVICTION_ADMISSION_MULTIPLIER") {
            if let Ok(mult) = multiplier.parse::<f64>() {
                self.eviction.admission_threshold_multiplier = mult;
                println!("Eviction admission multiplier from environment: {}", mult);
            }
        }

        if let Ok(adaptive) = std::env::var("CRABCACHE_EVICTION_ADAPTIVE") {
            if let Ok(enabled) = adaptive.parse::<bool>() {
                self.eviction.adaptive_eviction = enabled;
                println!("Adaptive eviction from environment: {}", enabled);
            }
        }
    }

    /// Get number of shards (CPU count if not specified)
    pub fn get_num_shards(&self) -> usize {
        self.num_shards.unwrap_or_else(|| num_cpus())
    }

    /// Update eviction configuration at runtime
    pub fn update_eviction_config(&mut self, new_config: EvictionConfig) -> Result<(), String> {
        new_config.validate()?;
        self.eviction = new_config;
        Ok(())
    }

    /// Get WAL configuration
    pub fn get_wal_config(&self) -> Result<WALConfig, String> {
        self.wal.to_wal_config(PathBuf::from(&self.wal_dir))
    }

    /// Validate security configuration
    fn validate_security_config(&self) -> Result<(), String> {
        if self.security.enable_auth && self.security.auth_token.is_none() {
            return Err("Authentication enabled but no auth token provided".to_string());
        }

        if self.security.enable_tls {
            if self.security.tls_cert_path.is_none() {
                return Err("TLS enabled but no certificate path provided".to_string());
            }
            if self.security.tls_key_path.is_none() {
                return Err("TLS enabled but no private key path provided".to_string());
            }
        }

        if self.security.max_command_size < 1024 {
            return Err("Max command size must be at least 1KB".to_string());
        }

        Ok(())
    }

    /// Validate rate limiting configuration
    fn validate_rate_limit_config(&self) -> Result<(), String> {
        if self.rate_limiting.enabled {
            if self.rate_limiting.max_requests_per_second == 0 {
                return Err("Max requests per second must be greater than 0".to_string());
            }

            if self.rate_limiting.burst_capacity == 0 {
                return Err("Burst capacity must be greater than 0".to_string());
            }

            if self.rate_limiting.window_seconds == 0 {
                return Err("Rate limit window must be greater than 0".to_string());
            }
        }

        Ok(())
    }

    /// Validate connection configuration
    fn validate_connection_config(&self) -> Result<(), String> {
        if self.connection.max_connections == 0 {
            return Err("Max connections must be greater than 0".to_string());
        }

        if self.connection.connection_timeout_seconds == 0 {
            return Err("Connection timeout must be greater than 0".to_string());
        }

        // Validate pipeline configuration
        if self.connection.pipeline.enabled {
            if self.connection.pipeline.max_batch_size == 0 {
                return Err("Pipeline max batch size must be greater than 0".to_string());
            }

            if self.connection.pipeline.max_batch_size > 1000 {
                return Err("Pipeline max batch size must not exceed 1000".to_string());
            }

            if self.connection.pipeline.buffer_size < 1024 {
                return Err("Pipeline buffer size must be at least 1KB".to_string());
            }

            if self.connection.pipeline.timeout_ms == 0 {
                return Err("Pipeline timeout must be greater than 0".to_string());
            }
        }

        Ok(())
    }

    /// Validate logging configuration
    fn validate_logging_config(&self) -> Result<(), String> {
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(format!(
                "Invalid log level: {}. Valid levels: {:?}",
                self.logging.level, valid_levels
            ));
        }

        let valid_formats = ["json", "pretty"];
        if !valid_formats.contains(&self.logging.format.as_str()) {
            return Err(format!(
                "Invalid log format: {}. Valid formats: {:?}",
                self.logging.format, valid_formats
            ));
        }

        if self.logging.enable_file && self.logging.file_path.is_none() {
            return Err("File logging enabled but no file path provided".to_string());
        }

        Ok(())
    }
    fn validate_wal_config(&self) -> Result<(), String> {
        if self.enable_wal {
            // Validate sync policy
            match self.wal.sync_policy.as_str() {
                "none" | "async" | "sync" => {}
                _ => return Err(format!("Invalid sync policy: {}", self.wal.sync_policy)),
            }

            // Validate segment size
            if self.wal.max_segment_size < 1024 * 1024 {
                // 1MB minimum
                return Err("WAL segment size must be at least 1MB".to_string());
            }

            // Validate buffer size
            if self.wal.buffer_size < 1024 {
                // 1KB minimum
                return Err("WAL buffer size must be at least 1KB".to_string());
            }
        }

        Ok(())
    }

    /// Save configuration to file
    pub async fn save(&self, path: &str) -> crate::Result<()> {
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content).await?;
        Ok(())
    }
}

// Simple CPU count implementation
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
