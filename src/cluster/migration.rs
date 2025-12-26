//! Shard migration and data rebalancing
//! 
//! This module handles the migration of data shards between nodes
//! during cluster rebalancing operations.

use crate::cluster::{NodeId, ClusterResult, ClusterError};
use crate::cluster::hash_ring::{ShardMigration, MigrationStatus};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, warn};

mod instant_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let now = SystemTime::now();
        let epoch_duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
        let millis = epoch_duration.as_millis() as u64;
        millis.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Instant, D::Error>
    where
        D: Deserializer<'de>,
    {
        let _millis = u64::deserialize(deserializer)?;
        Ok(Instant::now())
    }
}

mod option_instant_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(instant: &Option<Instant>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match instant {
            Some(_) => {
                let now = SystemTime::now();
                let epoch_duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
                let millis = epoch_duration.as_millis() as u64;
                Some(millis).serialize(serializer)
            }
            None => None::<u64>.serialize(serializer),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Instant>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = Option::<u64>::deserialize(deserializer)?;
        Ok(millis.map(|_| Instant::now()))
    }
}

/// Migration executor configuration
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Maximum concurrent migrations
    pub max_concurrent_migrations: u32,
    /// Batch size for data transfer
    pub batch_size: u32,
    /// Migration timeout
    pub migration_timeout: Duration,
    /// Retry attempts for failed migrations
    pub max_retries: u32,
    /// Throttling delay between batches
    pub throttle_delay: Duration,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            max_concurrent_migrations: 3,
            batch_size: 1000,
            migration_timeout: Duration::from_secs(300), // 5 minutes
            max_retries: 3,
            throttle_delay: Duration::from_millis(10),
        }
    }
}

/// Migration progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationProgress {
    pub migration_id: String,
    pub shard_id: crate::cluster::hash_ring::ShardId,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub status: MigrationStatus,
    pub progress_percentage: f64,
    pub keys_transferred: u64,
    pub total_keys: u64,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    #[serde(with = "instant_serde")]
    pub start_time: Instant,
    #[serde(with = "option_instant_serde")]
    pub estimated_completion: Option<Instant>,
    pub error_message: Option<String>,
}

impl MigrationProgress {
    /// Create new migration progress tracker
    pub fn new(migration: &ShardMigration) -> Self {
        Self {
            migration_id: migration.migration_id.clone(),
            shard_id: migration.shard_id,
            from_node: migration.from_node,
            to_node: migration.to_node,
            status: migration.status.clone(),
            progress_percentage: migration.progress * 100.0,
            keys_transferred: migration.transferred_keys,
            total_keys: migration.estimated_keys,
            bytes_transferred: 0,
            total_bytes: 0,
            start_time: migration.start_time,
            estimated_completion: None,
            error_message: None,
        }
    }
    
    /// Update progress
    pub fn update_progress(&mut self, keys_transferred: u64, bytes_transferred: u64) {
        self.keys_transferred = keys_transferred;
        self.bytes_transferred = bytes_transferred;
        
        if self.total_keys > 0 {
            self.progress_percentage = (keys_transferred as f64 / self.total_keys as f64) * 100.0;
        }
        
        // Estimate completion time based on current rate
        if keys_transferred > 0 && self.total_keys > keys_transferred {
            let elapsed = self.start_time.elapsed();
            let rate = keys_transferred as f64 / elapsed.as_secs_f64();
            let remaining_keys = self.total_keys - keys_transferred;
            let estimated_remaining_time = Duration::from_secs_f64(remaining_keys as f64 / rate);
            self.estimated_completion = Some(Instant::now() + estimated_remaining_time);
        }
    }
    
    /// Check if migration is complete
    pub fn is_complete(&self) -> bool {
        matches!(self.status, MigrationStatus::Completed)
    }
    
    /// Check if migration has failed
    pub fn has_failed(&self) -> bool {
        matches!(self.status, MigrationStatus::Failed)
    }
}

/// Data batch for migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationBatch {
    pub batch_id: String,
    pub migration_id: String,
    pub keys: Vec<String>,
    pub data: Vec<u8>,
    pub batch_index: u32,
    pub is_last_batch: bool,
}

/// Migration executor
pub struct MigrationExecutor {
    config: MigrationConfig,
    active_migrations: Arc<RwLock<HashMap<String, MigrationProgress>>>,
    migration_queue: Arc<RwLock<VecDeque<ShardMigration>>>,
    concurrency_limiter: Arc<Semaphore>,
    progress_sender: mpsc::UnboundedSender<MigrationProgress>,
    progress_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<MigrationProgress>>>>,
    metrics: Arc<RwLock<MigrationMetrics>>,
}

/// Migration metrics
#[derive(Debug, Clone, Default)]
pub struct MigrationMetrics {
    pub total_migrations: u64,
    pub completed_migrations: u64,
    pub failed_migrations: u64,
    pub active_migrations: u32,
    pub total_keys_migrated: u64,
    pub total_bytes_migrated: u64,
    pub avg_migration_time: f64,
    pub migration_throughput: f64, // keys per second
}

impl MigrationExecutor {
    /// Create new migration executor
    pub fn new(config: MigrationConfig) -> Self {
        let (progress_sender, progress_receiver) = mpsc::unbounded_channel();
        let concurrency_limiter = Arc::new(Semaphore::new(config.max_concurrent_migrations as usize));
        
        Self {
            config,
            active_migrations: Arc::new(RwLock::new(HashMap::new())),
            migration_queue: Arc::new(RwLock::new(VecDeque::new())),
            concurrency_limiter,
            progress_sender,
            progress_receiver: Arc::new(RwLock::new(Some(progress_receiver))),
            metrics: Arc::new(RwLock::new(MigrationMetrics::default())),
        }
    }
    
    /// Start migration executor
    pub async fn start(&self) -> ClusterResult<()> {
        info!("Starting migration executor");
        
        // Start migration processor
        self.start_migration_processor().await;
        
        // Start progress monitor
        self.start_progress_monitor().await;
        
        Ok(())
    }
    
    /// Queue migration for execution
    pub async fn queue_migration(&self, migration: ShardMigration) -> ClusterResult<()> {
        let mut queue = self.migration_queue.write().await;
        queue.push_back(migration.clone());
        
        info!("Queued migration {} for execution", migration.migration_id);
        
        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.total_migrations += 1;
        
        Ok(())
    }
    
    /// Start migration processor task
    async fn start_migration_processor(&self) {
        let queue = self.migration_queue.clone();
        let active_migrations = self.active_migrations.clone();
        let concurrency_limiter = self.concurrency_limiter.clone();
        let progress_sender = self.progress_sender.clone();
        let config = self.config.clone();
        let metrics = self.metrics.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100));
            
            loop {
                interval.tick().await;
                
                // Check if we can start new migrations
                if concurrency_limiter.available_permits() > 0 {
                    let migration = {
                        let mut queue_guard = queue.write().await;
                        queue_guard.pop_front()
                    };
                    
                    if let Some(migration) = migration {
                        let permit = concurrency_limiter.clone().acquire_owned().await.unwrap();
                        let active_migrations = active_migrations.clone();
                        let progress_sender = progress_sender.clone();
                        let config = config.clone();
                        let metrics = metrics.clone();
                        
                        tokio::spawn(async move {
                            let result = Self::execute_migration_task(
                                migration,
                                active_migrations,
                                progress_sender,
                                config,
                            ).await;
                            
                            // Update metrics
                            let mut metrics_guard = metrics.write().await;
                            match result {
                                Ok(_) => metrics_guard.completed_migrations += 1,
                                Err(_) => metrics_guard.failed_migrations += 1,
                            }
                            
                            drop(permit); // Release concurrency limit
                        });
                    }
                }
            }
        });
    }
    
    /// Execute a single migration
    async fn execute_migration_task(
        migration: ShardMigration,
        active_migrations: Arc<RwLock<HashMap<String, MigrationProgress>>>,
        progress_sender: mpsc::UnboundedSender<MigrationProgress>,
        config: MigrationConfig,
    ) -> ClusterResult<()> {
        let migration_id = migration.migration_id.clone();
        info!("Starting migration {}", migration_id);
        
        // Create progress tracker
        let mut progress = MigrationProgress::new(&migration);
        progress.status = MigrationStatus::InProgress;
        
        // Add to active migrations
        {
            let mut active = active_migrations.write().await;
            active.insert(migration_id.clone(), progress.clone());
        }
        
        // Send initial progress update
        let _ = progress_sender.send(progress.clone());
        
        // Execute migration with retries
        let mut attempts = 0;
        let mut last_error = None;
        
        while attempts < config.max_retries {
            attempts += 1;
            
            match Self::perform_migration(&migration, &mut progress, &progress_sender, &config).await {
                Ok(_) => {
                    progress.status = MigrationStatus::Completed;
                    progress.progress_percentage = 100.0;
                    
                    // Update active migrations
                    {
                        let mut active = active_migrations.write().await;
                        active.insert(migration_id.clone(), progress.clone());
                    }
                    
                    let _ = progress_sender.send(progress.clone());
                    info!("Migration {} completed successfully", migration_id);
                    
                    // Remove from active migrations
                    {
                        let mut active = active_migrations.write().await;
                        active.remove(&migration_id);
                    }
                    
                    return Ok(());
                }
                Err(e) => {
                    last_error = Some(e);
                    warn!("Migration {} attempt {} failed: {:?}", migration_id, attempts, last_error);
                    
                    if attempts < config.max_retries {
                        // Wait before retry
                        sleep(Duration::from_secs(attempts as u64 * 2)).await;
                    }
                }
            }
        }
        
        // Migration failed after all retries
        progress.status = MigrationStatus::Failed;
        progress.error_message = last_error.as_ref().map(|e| e.to_string());
        
        {
            let mut active = active_migrations.write().await;
            active.insert(migration_id.clone(), progress.clone());
        }
        
        let _ = progress_sender.send(progress);
        error!("Migration {} failed after {} attempts", migration_id, config.max_retries);
        
        // Remove from active migrations
        {
            let mut active = active_migrations.write().await;
            active.remove(&migration_id);
        }
        
        Err(last_error.unwrap_or_else(|| ClusterError::MigrationFailed { 
            migration_id: migration_id.clone() 
        }))
    }
    
    /// Perform the actual migration
    async fn perform_migration(
        migration: &ShardMigration,
        progress: &mut MigrationProgress,
        progress_sender: &mpsc::UnboundedSender<MigrationProgress>,
        config: &MigrationConfig,
    ) -> ClusterResult<()> {
        // Phase 1: Scan and count keys
        info!("Phase 1: Scanning keys for migration {}", migration.migration_id);
        let total_keys = Self::scan_keys_for_migration(migration).await?;
        progress.total_keys = total_keys;
        
        // Phase 2: Transfer data in batches
        info!("Phase 2: Transferring {} keys for migration {}", total_keys, migration.migration_id);
        let mut transferred_keys = 0;
        let mut batch_index = 0;
        
        while transferred_keys < total_keys {
            let batch_size = config.batch_size.min((total_keys - transferred_keys) as u32) as usize;
            
            // Create migration batch
            let batch = Self::create_migration_batch(
                migration,
                batch_index,
                batch_size,
                transferred_keys == total_keys - batch_size as u64,
            ).await?;
            
            // Transfer batch
            Self::transfer_batch(&batch, migration).await?;
            
            transferred_keys += batch_size as u64;
            batch_index += 1;
            
            // Update progress
            progress.update_progress(transferred_keys, 0); // TODO: Track bytes
            let _ = progress_sender.send(progress.clone());
            
            // Throttle to avoid overwhelming the network
            sleep(config.throttle_delay).await;
        }
        
        // Phase 3: Verify migration
        info!("Phase 3: Verifying migration {}", migration.migration_id);
        Self::verify_migration(migration).await?;
        
        // Phase 4: Cleanup source data
        info!("Phase 4: Cleaning up source data for migration {}", migration.migration_id);
        Self::cleanup_source_data(migration).await?;
        
        Ok(())
    }
    
    /// Scan keys that need to be migrated
    async fn scan_keys_for_migration(migration: &ShardMigration) -> ClusterResult<u64> {
        // TODO: Implement actual key scanning
        // This would scan the source node for keys in the specified range
        debug!("Scanning keys for migration {} in range {:?}", 
               migration.migration_id, migration.key_range);
        
        // Simulate key count
        Ok(migration.estimated_keys.max(1000))
    }
    
    /// Create a batch of data for migration
    async fn create_migration_batch(
        migration: &ShardMigration,
        batch_index: u32,
        batch_size: usize,
        is_last_batch: bool,
    ) -> ClusterResult<MigrationBatch> {
        // TODO: Implement actual batch creation
        // This would read data from the source node
        
        let batch_id = format!("{}_{}", migration.migration_id, batch_index);
        let keys: Vec<String> = (0..batch_size)
            .map(|i| format!("key_{}_{}", batch_index, i))
            .collect();
        
        Ok(MigrationBatch {
            batch_id,
            migration_id: migration.migration_id.clone(),
            keys,
            data: vec![0; batch_size * 100], // Simulate data
            batch_index,
            is_last_batch,
        })
    }
    
    /// Transfer a batch to the target node
    async fn transfer_batch(batch: &MigrationBatch, migration: &ShardMigration) -> ClusterResult<()> {
        // TODO: Implement actual batch transfer
        // This would send the batch to the target node
        debug!("Transferring batch {} ({} keys) from {} to {}", 
               batch.batch_id, batch.keys.len(), migration.from_node, migration.to_node);
        
        // Simulate network delay
        sleep(Duration::from_millis(10)).await;
        
        Ok(())
    }
    
    /// Verify migration completed successfully
    async fn verify_migration(migration: &ShardMigration) -> ClusterResult<()> {
        // TODO: Implement migration verification
        // This would check that all data was transferred correctly
        debug!("Verifying migration {}", migration.migration_id);
        
        Ok(())
    }
    
    /// Cleanup source data after successful migration
    async fn cleanup_source_data(migration: &ShardMigration) -> ClusterResult<()> {
        // TODO: Implement source cleanup
        // This would remove the migrated data from the source node
        debug!("Cleaning up source data for migration {}", migration.migration_id);
        
        Ok(())
    }
    
    /// Start progress monitor task
    async fn start_progress_monitor(&self) {
        let mut progress_receiver = self.progress_receiver.write().await.take()
            .expect("Progress receiver should be available");
        let metrics = self.metrics.clone();
        
        tokio::spawn(async move {
            while let Some(progress) = progress_receiver.recv().await {
                debug!("Migration {} progress: {:.1}%", 
                       progress.migration_id, progress.progress_percentage);
                
                // Update metrics
                if progress.is_complete() {
                    let mut metrics_guard = metrics.write().await;
                    metrics_guard.total_keys_migrated += progress.keys_transferred;
                    metrics_guard.total_bytes_migrated += progress.bytes_transferred;
                    
                    let migration_time = progress.start_time.elapsed().as_secs_f64();
                    if metrics_guard.avg_migration_time == 0.0 {
                        metrics_guard.avg_migration_time = migration_time;
                    } else {
                        metrics_guard.avg_migration_time = 
                            (metrics_guard.avg_migration_time * 0.9) + (migration_time * 0.1);
                    }
                    
                    if migration_time > 0.0 {
                        let throughput = progress.keys_transferred as f64 / migration_time;
                        if metrics_guard.migration_throughput == 0.0 {
                            metrics_guard.migration_throughput = throughput;
                        } else {
                            metrics_guard.migration_throughput = 
                                (metrics_guard.migration_throughput * 0.9) + (throughput * 0.1);
                        }
                    }
                }
            }
        });
    }
    
    /// Get active migrations
    pub async fn get_active_migrations(&self) -> HashMap<String, MigrationProgress> {
        self.active_migrations.read().await.clone()
    }
    
    /// Get migration progress
    pub async fn get_migration_progress(&self, migration_id: &str) -> Option<MigrationProgress> {
        self.active_migrations.read().await.get(migration_id).cloned()
    }
    
    /// Cancel migration
    pub async fn cancel_migration(&self, migration_id: &str) -> ClusterResult<()> {
        let mut active = self.active_migrations.write().await;
        if let Some(progress) = active.get_mut(migration_id) {
            progress.status = MigrationStatus::Cancelled;
            info!("Cancelled migration {}", migration_id);
            Ok(())
        } else {
            Err(ClusterError::MigrationFailed { 
                migration_id: migration_id.to_string() 
            })
        }
    }
    
    /// Get migration metrics
    pub async fn get_metrics(&self) -> MigrationMetrics {
        let mut metrics = self.metrics.read().await.clone();
        metrics.active_migrations = self.active_migrations.read().await.len() as u32;
        metrics
    }
    
    /// Get queued migrations count
    pub async fn get_queued_count(&self) -> usize {
        self.migration_queue.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::node::NodeId;
    
    #[tokio::test]
    async fn test_migration_executor_creation() {
        let config = MigrationConfig::default();
        let executor = MigrationExecutor::new(config);
        
        let metrics = executor.get_metrics().await;
        assert_eq!(metrics.total_migrations, 0);
        assert_eq!(metrics.active_migrations, 0);
    }
    
    #[tokio::test]
    async fn test_migration_queue() {
        let config = MigrationConfig::default();
        let executor = MigrationExecutor::new(config);
        
        let migration = ShardMigration {
            migration_id: "test_migration".to_string(),
            shard_id: crate::cluster::hash_ring::ShardId::new(1),
            from_node: NodeId::generate(),
            to_node: NodeId::generate(),
            key_range: (0, 1000),
            status: MigrationStatus::Planned,
            progress: 0.0,
            estimated_keys: 1000,
            transferred_keys: 0,
            start_time: Instant::now(),
        };
        
        executor.queue_migration(migration).await.unwrap();
        
        assert_eq!(executor.get_queued_count().await, 1);
        
        let metrics = executor.get_metrics().await;
        assert_eq!(metrics.total_migrations, 1);
    }
    
    #[test]
    fn test_migration_progress() {
        let migration = ShardMigration {
            migration_id: "test_migration".to_string(),
            shard_id: crate::cluster::hash_ring::ShardId::new(1),
            from_node: NodeId::generate(),
            to_node: NodeId::generate(),
            key_range: (0, 1000),
            status: MigrationStatus::InProgress,
            progress: 0.5,
            estimated_keys: 1000,
            transferred_keys: 500,
            start_time: Instant::now(),
        };
        
        let mut progress = MigrationProgress::new(&migration);
        assert_eq!(progress.progress_percentage, 50.0);
        assert_eq!(progress.keys_transferred, 500);
        assert_eq!(progress.total_keys, 1000);
        
        // Update progress
        progress.update_progress(750, 75000);
        assert_eq!(progress.progress_percentage, 75.0);
        assert_eq!(progress.keys_transferred, 750);
        assert_eq!(progress.bytes_transferred, 75000);
    }
    
    #[test]
    fn test_migration_batch() {
        let batch = MigrationBatch {
            batch_id: "batch_1".to_string(),
            migration_id: "migration_1".to_string(),
            keys: vec!["key1".to_string(), "key2".to_string()],
            data: vec![1, 2, 3, 4],
            batch_index: 0,
            is_last_batch: false,
        };
        
        assert_eq!(batch.keys.len(), 2);
        assert_eq!(batch.data.len(), 4);
        assert!(!batch.is_last_batch);
    }
    
    #[test]
    fn test_migration_config() {
        let config = MigrationConfig::default();
        
        assert_eq!(config.max_concurrent_migrations, 3);
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.max_retries, 3);
        assert!(config.migration_timeout > Duration::from_secs(0));
    }
    
    #[test]
    fn test_migration_metrics() {
        let mut metrics = MigrationMetrics::default();
        
        assert_eq!(metrics.total_migrations, 0);
        assert_eq!(metrics.completed_migrations, 0);
        assert_eq!(metrics.failed_migrations, 0);
        
        metrics.total_migrations = 10;
        metrics.completed_migrations = 8;
        metrics.failed_migrations = 2;
        
        assert_eq!(metrics.total_migrations, 10);
        assert_eq!(metrics.completed_migrations, 8);
        assert_eq!(metrics.failed_migrations, 2);
    }
}