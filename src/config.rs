//! Configuration management for CrabCache - Binary Fixes Version

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable authentication
    pub enable_auth: bool,
    /// Authentication token
    pub auth_token: Option<String>,
    /// Allowed IP addresses (empty = allow all)
    pub allowed_ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,
    /// Maximum requests per second per client
    pub max_requests_per_second: u32,
    /// Burst capacity
    pub burst_capacity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Connection timeout in seconds
    pub timeout_seconds: u64,
    /// Enable TCP keepalive
    pub keepalive: bool,
    /// Pipeline configuration
    pub pipeline: PipelineConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Enable pipelining
    pub enabled: bool,
    /// Maximum batch size for pipelining
    pub max_batch_size: usize,
    /// Batch timeout in milliseconds
    pub batch_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (error, warn, info, debug, trace)
    pub level: String,
    /// Log format (json, pretty)
    pub format: String,
    /// Enable file logging
    pub file_enabled: bool,
    /// Log file path
    pub file_path: String,
    /// Maximum log file size in MB
    pub max_file_size_mb: u64,
    /// Number of log files to keep
    pub max_files: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WALConfigToml {
    /// Enable WAL
    pub enabled: bool,
    /// WAL directory
    pub dir: String,
    /// Sync policy
    pub sync_policy: String,
    /// Sync interval in milliseconds (for Interval policy)
    pub sync_interval_ms: u64,
    /// Maximum segment size in bytes
    pub max_segment_size: usize,
    /// Enable compression
    pub enable_compression: bool,
    /// Compression level (1-9)
    pub compression_level: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1".to_string(),
            port: 8000,
            num_shards: None,
            max_memory_per_shard: 256 * 1024 * 1024, // 256MB
            enable_wal: false,
            wal_dir: "./data/wal".to_string(),
            wal: WALConfigToml::default(),
            metrics: MetricsConfig::default(),
            eviction: EvictionConfig::default(),
            security: SecurityConfig::default(),
            rate_limiting: RateLimitConfig::default(),
            connection: ConnectionConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: "/metrics".to_string(),
            port: 9090,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_auth: false,
            auth_token: None,
            allowed_ips: vec![],
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_requests_per_second: 1000,
            burst_capacity: 100,
        }
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_connections: 1000,
            timeout_seconds: 30,
            keepalive: true,
            pipeline: PipelineConfig::default(),
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_batch_size: 16,
            batch_timeout_ms: 10,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
            file_enabled: false,
            file_path: "./logs/crabcache.log".to_string(),
            max_file_size_mb: 100,
            max_files: 5,
        }
    }
}

impl Default for WALConfigToml {
    fn default() -> Self {
        Self {
            enabled: false,
            dir: "./data/wal".to_string(),
            sync_policy: "Async".to_string(),
            sync_interval_ms: 1000,
            max_segment_size: 64 * 1024 * 1024, // 64MB
            enable_compression: true,
            compression_level: 6,
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

    /// Get number of shards (defaults to CPU count)
    pub fn get_num_shards(&self) -> usize {
        self.num_shards.unwrap_or_else(num_cpus::get)
    }

    /// Get WAL configuration
    pub fn get_wal_config(&self) -> Result<WALConfig, String> {
        if !self.wal.enabled {
            return Err("WAL is not enabled".to_string());
        }

        let sync_policy = match self.wal.sync_policy.as_str() {
            "None" => SyncPolicy::None,
            "Async" => SyncPolicy::Async,
            "Sync" => SyncPolicy::Sync,
            _ => return Err(format!("Invalid sync policy: {}", self.wal.sync_policy)),
        };

        Ok(WALConfig {
            wal_dir: PathBuf::from(&self.wal.dir),
            max_segment_size: self.wal.max_segment_size as u64,
            buffer_size: 4096, // Default buffer size
            flush_interval_ms: self.wal.sync_interval_ms,
            sync_policy,
        })
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        if let Ok(bind_addr) = std::env::var("CRABCACHE_BIND_ADDR") {
            self.bind_addr = bind_addr;
            println!("Bind address from environment: {}", self.bind_addr);
        }

        if let Ok(port) = std::env::var("CRABCACHE_PORT") {
            if let Ok(port_num) = port.parse::<u16>() {
                self.port = port_num;
                println!("Port from environment: {}", self.port);
            }
        }

        // Pipeline configuration
        if let Ok(enable_pipeline) = std::env::var("CRABCACHE_ENABLE_PIPELINING") {
            if let Ok(enabled) = enable_pipeline.parse::<bool>() {
                self.connection.pipeline.enabled = enabled;
                println!("Pipelining enabled from environment: {}", enabled);
            }
        }

        if let Ok(batch_size) = std::env::var("CRABCACHE_MAX_BATCH_SIZE") {
            if let Ok(size) = batch_size.parse::<usize>() {
                self.connection.pipeline.max_batch_size = size;
                println!("Max batch size from environment: {}", size);
            }
        }
    }

    /// Validate WAL configuration
    fn validate_wal_config(&self) -> Result<(), String> {
        if self.enable_wal && self.wal_dir.is_empty() {
            return Err("WAL directory cannot be empty when WAL is enabled".to_string());
        }
        Ok(())
    }

    /// Validate security configuration
    fn validate_security_config(&self) -> Result<(), String> {
        if self.security.enable_auth && self.security.auth_token.is_none() {
            return Err("Auth token is required when authentication is enabled".to_string());
        }
        Ok(())
    }

    /// Validate rate limiting configuration
    fn validate_rate_limit_config(&self) -> Result<(), String> {
        if self.rate_limiting.enabled {
            if self.rate_limiting.max_requests_per_second == 0 {
                return Err(
                    "Max requests per second cannot be 0 when rate limiting is enabled".to_string(),
                );
            }
            if self.rate_limiting.burst_capacity == 0 {
                return Err("Burst capacity cannot be 0 when rate limiting is enabled".to_string());
            }
        }
        Ok(())
    }

    /// Validate connection configuration
    fn validate_connection_config(&self) -> Result<(), String> {
        if self.connection.max_connections == 0 {
            return Err("Max connections cannot be 0".to_string());
        }
        if self.connection.timeout_seconds == 0 {
            return Err("Connection timeout cannot be 0".to_string());
        }
        if self.connection.pipeline.max_batch_size == 0 {
            return Err("Pipeline max batch size cannot be 0".to_string());
        }
        Ok(())
    }

    /// Validate logging configuration
    fn validate_logging_config(&self) -> Result<(), String> {
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(format!("Invalid log level: {}", self.logging.level));
        }

        let valid_formats = ["json", "pretty"];
        if !valid_formats.contains(&self.logging.format.as_str()) {
            return Err(format!("Invalid log format: {}", self.logging.format));
        }

        if self.logging.file_enabled && self.logging.file_path.is_empty() {
            return Err("Log file path cannot be empty when file logging is enabled".to_string());
        }

        Ok(())
    }
}
