//! WAL-enabled shard manager with persistence

use super::eviction_manager::EvictionShardManager;
use crate::eviction::EvictionConfig;
use crate::protocol::commands::{Command, Response};
use crate::wal::{Operation, RecoveryStats, WALConfig, WALReader, WALReplayTarget, WALWriter};
use bytes::Bytes;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Shard manager with WAL persistence and eviction
pub struct WALShardManager {
    /// Base eviction manager
    eviction_manager: EvictionShardManager,
    /// WAL writer for persistence
    wal_writer: Option<Arc<WALWriter>>,
    /// WAL configuration
    wal_config: Option<WALConfig>,
    /// Whether WAL is enabled
    wal_enabled: bool,
}

impl WALShardManager {
    /// Create new WAL-enabled shard manager
    pub async fn new(
        num_shards: usize,
        max_memory_per_shard: usize,
        eviction_config: EvictionConfig,
        wal_config: Option<WALConfig>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        info!("Creating WAL-enabled shard manager");

        // Create base eviction manager
        let eviction_manager =
            EvictionShardManager::new(num_shards, max_memory_per_shard, eviction_config)?;

        let (wal_writer, wal_enabled) = if let Some(config) = &wal_config {
            info!("Initializing WAL with config: {:?}", config);
            let writer = WALWriter::new(config.clone()).await?;
            (Some(Arc::new(writer)), true)
        } else {
            info!("WAL disabled");
            (None, false)
        };

        Ok(Self {
            eviction_manager,
            wal_writer,
            wal_config,
            wal_enabled,
        })
    }

    /// Create WAL-enabled manager with recovery
    pub async fn new_with_recovery(
        num_shards: usize,
        max_memory_per_shard: usize,
        eviction_config: EvictionConfig,
        wal_config: Option<WALConfig>,
    ) -> Result<(Self, Option<RecoveryStats>), Box<dyn std::error::Error + Send + Sync>> {
        let manager = Self::new(
            num_shards,
            max_memory_per_shard,
            eviction_config,
            wal_config,
        )
        .await?;

        let recovery_stats = if manager.wal_enabled {
            Some(manager.recover_from_wal().await?)
        } else {
            None
        };

        Ok((manager, recovery_stats))
    }

    /// Recover from WAL segments
    async fn recover_from_wal(
        &self,
    ) -> Result<RecoveryStats, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref config) = self.wal_config {
            info!("Starting WAL recovery");
            let reader = WALReader::new(&config.wal_dir);
            let stats = reader.replay_to_manager(self).await?;
            info!("WAL recovery completed: {:?}", stats);
            Ok(stats)
        } else {
            Ok(RecoveryStats::default())
        }
    }

    /// Process command with WAL logging
    pub async fn process_command(&self, command: Command) -> Response {
        // Log to WAL before processing (if enabled)
        if self.wal_enabled {
            if let Err(e) = self.log_command_to_wal(&command).await {
                warn!("Failed to log command to WAL: {}", e);
                // Continue processing even if WAL fails (availability over durability)
            }
        }

        // Process command through eviction manager
        self.eviction_manager.process_command(command).await
    }

    /// Log command to WAL
    async fn log_command_to_wal(
        &self,
        command: &Command,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref wal_writer) = self.wal_writer {
            let shard_id = match command {
                Command::Put { key, .. }
                | Command::Get { key }
                | Command::Del { key }
                | Command::Expire { key, .. } => self.get_shard_id(key),
                _ => return Ok(()), // Skip non-key commands
            };

            let operation = self.command_to_operation(command)?;

            // Use async write for better performance
            wal_writer.write_operation_async(shard_id, operation)?;
        }
        Ok(())
    }

    /// Convert command to WAL operation
    fn command_to_operation(&self, command: &Command) -> Result<Operation, String> {
        match command {
            Command::Put { key, value, ttl } => Ok(Operation::Put {
                key: String::from_utf8_lossy(key).to_string(),
                value: value.to_vec(),
                ttl: *ttl,
            }),
            Command::Del { key } => Ok(Operation::Delete {
                key: String::from_utf8_lossy(key).to_string(),
            }),
            Command::Expire { key, ttl } => Ok(Operation::Expire {
                key: String::from_utf8_lossy(key).to_string(),
                ttl: *ttl,
            }),
            _ => Err("Command cannot be logged to WAL".to_string()),
        }
    }

    /// Get shard ID for key
    fn get_shard_id(&self, key: &Bytes) -> usize {
        self.eviction_manager
            .get_shard_id(&String::from_utf8_lossy(key))
    }

    /// Get metrics (delegate to eviction manager)
    pub async fn get_metrics(&self) -> serde_json::Value {
        let mut metrics = self.eviction_manager.get_metrics().await;

        // Add WAL-specific metrics
        if self.wal_enabled {
            let wal_metrics = serde_json::json!({
                "wal_enabled": true,
                "wal_config": {
                    "max_segment_size": self.wal_config.as_ref().map(|c| c.max_segment_size),
                    "buffer_size": self.wal_config.as_ref().map(|c| c.buffer_size),
                    "flush_interval_ms": self.wal_config.as_ref().map(|c| c.flush_interval_ms),
                    "sync_policy": self.wal_config.as_ref().map(|c| format!("{:?}", c.sync_policy)),
                }
            });

            if let Some(obj) = metrics.as_object_mut() {
                obj.insert("wal".to_string(), wal_metrics);
            }
        } else {
            let wal_metrics = serde_json::json!({
                "wal_enabled": false
            });

            if let Some(obj) = metrics.as_object_mut() {
                obj.insert("wal".to_string(), wal_metrics);
            }
        }

        metrics
    }

    /// Force flush WAL
    pub async fn flush_wal(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref wal_writer) = self.wal_writer {
            wal_writer.flush().await?;
        }
        Ok(())
    }

    /// Get number of shards
    pub fn num_shards(&self) -> usize {
        self.eviction_manager.num_shards()
    }

    /// Shutdown gracefully
    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Shutting down WAL shard manager");

        // Flush WAL before shutdown
        if let Err(e) = self.flush_wal().await {
            warn!("Failed to flush WAL during shutdown: {}", e);
        }

        // Shutdown eviction manager
        self.eviction_manager.shutdown().await?;

        info!("WAL shard manager shutdown complete");
        Ok(())
    }
}

/// Implement WAL replay target for recovery
#[async_trait::async_trait]
impl WALReplayTarget for WALShardManager {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    async fn replay_operation(
        &self,
        shard_id: usize,
        operation: &Operation,
    ) -> Result<(), Self::Error> {
        debug!(
            "Replaying WAL operation: shard={}, op={:?}",
            shard_id, operation
        );

        let command = match operation {
            Operation::Put { key, value, ttl } => Command::Put {
                key: bytes::Bytes::from(key.clone()),
                value: bytes::Bytes::from(value.clone()),
                ttl: *ttl,
            },
            Operation::Delete { key } => Command::Del {
                key: bytes::Bytes::from(key.clone()),
            },
            Operation::Expire { key, ttl } => Command::Expire {
                key: bytes::Bytes::from(key.clone()),
                ttl: *ttl,
            },
        };

        // Process command without WAL logging (to avoid infinite loop)
        let _response = self.eviction_manager.process_command(command).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::SyncPolicy;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_wal_manager_basic() {
        let temp_dir = TempDir::new().unwrap();
        let wal_config = WALConfig {
            wal_dir: temp_dir.path().to_path_buf(),
            max_segment_size: 1024,
            buffer_size: 256,
            flush_interval_ms: 100,
            sync_policy: SyncPolicy::Async,
        };

        let eviction_config = EvictionConfig::default();

        let manager = WALShardManager::new(2, 1024 * 1024, eviction_config, Some(wal_config))
            .await
            .unwrap();

        // Test basic operations
        let put_cmd = Command::Put {
            key: "test_key".to_string().into(),
            value: bytes::Bytes::from("test_value"),
            ttl: None,
        };

        let response = manager.process_command(put_cmd).await;
        assert!(matches!(response, Response::Ok));

        // Flush WAL
        manager.flush_wal().await.unwrap();
    }

    #[tokio::test]
    async fn test_wal_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let wal_config = WALConfig {
            wal_dir: temp_dir.path().to_path_buf(),
            max_segment_size: 1024,
            buffer_size: 256,
            flush_interval_ms: 100,
            sync_policy: SyncPolicy::Sync,
        };

        let eviction_config = EvictionConfig::default();

        // Create manager and write some data
        {
            let manager = WALShardManager::new(
                2,
                1024 * 1024,
                eviction_config.clone(),
                Some(wal_config.clone()),
            )
            .await
            .unwrap();

            let put_cmd = Command::Put {
                key: "recovery_test".to_string().into(),
                value: bytes::Bytes::from("recovery_value"),
                ttl: None,
            };

            manager.process_command(put_cmd).await;
            manager.flush_wal().await.unwrap();
        }

        // Create new manager with recovery
        let (manager, stats) =
            WALShardManager::new_with_recovery(2, 1024 * 1024, eviction_config, Some(wal_config))
                .await
                .unwrap();

        assert!(stats.is_some());
        let stats = stats.unwrap();
        assert!(stats.entries_recovered > 0);

        // Verify data was recovered
        let get_cmd = Command::Get {
            key: "recovery_test".to_string().into(),
        };

        let response = manager.process_command(get_cmd).await;
        match response {
            Response::Value(value) => {
                assert_eq!(value, bytes::Bytes::from("recovery_value"));
            }
            _ => panic!("Expected value response"),
        }
    }
}
