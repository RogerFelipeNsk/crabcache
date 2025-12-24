//! WAL writer implementation

use crate::wal::entry::{Operation, SegmentHeader, WALEntry};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

/// WAL writer configuration
#[derive(Debug, Clone)]
pub struct WALConfig {
    /// Directory to store WAL segments
    pub wal_dir: PathBuf,
    /// Maximum segment size in bytes (default: 64MB)
    pub max_segment_size: u64,
    /// Buffer size for batching writes (default: 4KB)
    pub buffer_size: usize,
    /// Flush interval in milliseconds (default: 1000ms)
    pub flush_interval_ms: u64,
    /// Sync policy for durability
    pub sync_policy: SyncPolicy,
}

/// Sync policy for WAL durability
#[derive(Debug, Clone, Copy)]
pub enum SyncPolicy {
    /// No explicit sync (fastest, least durable)
    None,
    /// Async sync (balanced)
    Async,
    /// Sync after each write (slowest, most durable)
    Sync,
}

impl Default for WALConfig {
    fn default() -> Self {
        Self {
            wal_dir: PathBuf::from("./data/wal"),
            max_segment_size: 64 * 1024 * 1024, // 64MB
            buffer_size: 4096,                  // 4KB
            flush_interval_ms: 1000,            // 1 second
            sync_policy: SyncPolicy::Async,
        }
    }
}

/// Write-Ahead Log writer with segmentation and buffering
pub struct WALWriter {
    config: WALConfig,
    current_segment: Arc<Mutex<Option<SegmentWriter>>>,
    write_tx: mpsc::UnboundedSender<WriteRequest>,
    _background_task: tokio::task::JoinHandle<()>,
}

/// Internal segment writer
struct SegmentWriter {
    file: BufWriter<File>,
    path: PathBuf,
    header: SegmentHeader,
    current_size: u64,
    entry_count: u64,
}

/// Write request for background processing
#[derive(Debug)]
struct WriteRequest {
    entry: WALEntry,
    response_tx: Option<tokio::sync::oneshot::Sender<Result<(), WALError>>>,
}

/// WAL-specific errors
#[derive(Debug, thiserror::Error)]
pub enum WALError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),
    #[error("WAL directory creation failed: {0}")]
    DirectoryCreation(String),
    #[error("Segment rotation failed: {0}")]
    SegmentRotation(String),
    #[error("Channel closed")]
    ChannelClosed,
}

impl WALWriter {
    /// Create new WAL writer
    pub async fn new(config: WALConfig) -> Result<Self, WALError> {
        // Ensure WAL directory exists
        if !config.wal_dir.exists() {
            std::fs::create_dir_all(&config.wal_dir)
                .map_err(|e| WALError::DirectoryCreation(e.to_string()))?;
            info!("Created WAL directory: {:?}", config.wal_dir);
        }

        let (write_tx, write_rx) = mpsc::unbounded_channel();
        let current_segment = Arc::new(Mutex::new(None));

        // Start background writer task
        let background_task = tokio::spawn(Self::background_writer(
            config.clone(),
            current_segment.clone(),
            write_rx,
        ));

        Ok(Self {
            config,
            current_segment,
            write_tx,
            _background_task: background_task,
        })
    }

    /// Write operation to WAL (async, non-blocking)
    pub async fn write_operation(
        &self,
        shard_id: usize,
        operation: Operation,
    ) -> Result<(), WALError> {
        let entry = WALEntry::new(shard_id, operation);

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = WriteRequest {
            entry,
            response_tx: Some(response_tx),
        };

        self.write_tx
            .send(request)
            .map_err(|_| WALError::ChannelClosed)?;

        response_rx.await.map_err(|_| WALError::ChannelClosed)?
    }

    /// Write operation to WAL (fire-and-forget)
    pub fn write_operation_async(
        &self,
        shard_id: usize,
        operation: Operation,
    ) -> Result<(), WALError> {
        let entry = WALEntry::new(shard_id, operation);

        let request = WriteRequest {
            entry,
            response_tx: None,
        };

        self.write_tx
            .send(request)
            .map_err(|_| WALError::ChannelClosed)?;

        Ok(())
    }

    /// Force flush all pending writes
    pub async fn flush(&self) -> Result<(), WALError> {
        // Create a oneshot channel for the flush response
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        // Send a special flush request
        let flush_entry = WALEntry::new(
            usize::MAX,
            Operation::Put {
                key: "__flush__".to_string(),
                value: vec![],
                ttl: None,
            },
        );

        let request = WriteRequest {
            entry: flush_entry,
            response_tx: Some(response_tx),
        };

        self.write_tx
            .send(request)
            .map_err(|_| WALError::ChannelClosed)?;

        response_rx.await.map_err(|_| WALError::ChannelClosed)?
    }

    /// Background writer task
    async fn background_writer(
        config: WALConfig,
        current_segment: Arc<Mutex<Option<SegmentWriter>>>,
        mut write_rx: mpsc::UnboundedReceiver<WriteRequest>,
    ) {
        let mut flush_timer = interval(Duration::from_millis(config.flush_interval_ms));
        let mut pending_writes = Vec::new();

        loop {
            tokio::select! {
                // Handle write requests
                request = write_rx.recv() => {
                    match request {
                        Some(req) => {
                            // Check for flush requests
                            if req.entry.shard_id == usize::MAX &&
                               matches!(req.entry.operation, Operation::Put { ref key, .. } if key == "__flush__") {
                                // This is a flush request, process pending writes and respond
                                Self::process_writes(&config, &current_segment, &mut pending_writes).await;
                                if let Some(response_tx) = req.response_tx {
                                    let _ = response_tx.send(Ok(()));
                                }
                            } else {
                                // Regular write request
                                pending_writes.push(req);

                                // Batch writes for efficiency
                                if pending_writes.len() >= 10 {
                                    Self::process_writes(&config, &current_segment, &mut pending_writes).await;
                                }
                            }
                        }
                        None => {
                            // Channel closed, flush remaining writes and exit
                            Self::process_writes(&config, &current_segment, &mut pending_writes).await;
                            break;
                        }
                    }
                }

                // Periodic flush
                _ = flush_timer.tick() => {
                    if !pending_writes.is_empty() {
                        Self::process_writes(&config, &current_segment, &mut pending_writes).await;
                    }
                }
            }
        }

        info!("WAL writer background task terminated");
    }

    /// Process batch of writes
    async fn process_writes(
        config: &WALConfig,
        current_segment: &Arc<Mutex<Option<SegmentWriter>>>,
        pending_writes: &mut Vec<WriteRequest>,
    ) {
        if pending_writes.is_empty() {
            return;
        }

        let mut segment_guard = current_segment.lock().await;

        for request in pending_writes.drain(..) {
            let result =
                Self::write_entry_to_segment(config, &mut segment_guard, &request.entry).await;

            if let Some(response_tx) = request.response_tx {
                let _ = response_tx.send(result);
            }
        }

        // Flush the segment
        if let Some(ref mut segment) = *segment_guard {
            if let Err(e) = segment.flush(config.sync_policy) {
                error!("Failed to flush WAL segment: {}", e);
            }
        }
    }

    /// Write single entry to current segment
    async fn write_entry_to_segment(
        config: &WALConfig,
        current_segment: &mut Option<SegmentWriter>,
        entry: &WALEntry,
    ) -> Result<(), WALError> {
        // Ensure we have a current segment
        if current_segment.is_none()
            || current_segment
                .as_ref()
                .unwrap()
                .needs_rotation(config.max_segment_size)
        {
            *current_segment = Some(Self::create_new_segment(config).await?);
        }

        let segment = current_segment.as_mut().unwrap();
        segment.write_entry(entry)
    }

    /// Create new WAL segment
    async fn create_new_segment(config: &WALConfig) -> Result<SegmentWriter, WALError> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();

        let filename = format!("wal-{:016x}.log", timestamp);
        let path = config.wal_dir.join(filename);

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(false)
            .open(&path)?;

        let mut segment = SegmentWriter {
            file: BufWriter::with_capacity(config.buffer_size, file),
            path: path.clone(),
            header: SegmentHeader::new(),
            current_size: 0,
            entry_count: 0,
        };

        // Write header placeholder (will be updated on close)
        let header_bytes = segment.header.serialize()?;
        let header_len = header_bytes.len() as u32;

        segment.file.write_all(&header_len.to_le_bytes())?;
        segment.file.write_all(&header_bytes)?;
        segment.current_size = 4 + header_bytes.len() as u64;

        info!("Created new WAL segment: {:?}", path);
        Ok(segment)
    }
}

impl SegmentWriter {
    /// Write entry to segment
    fn write_entry(&mut self, entry: &WALEntry) -> Result<(), WALError> {
        let entry_bytes = entry.serialize()?;
        let entry_len = entry_bytes.len() as u32;

        // Write length prefix + entry
        self.file.write_all(&entry_len.to_le_bytes())?;
        self.file.write_all(&entry_bytes)?;

        self.current_size += 4 + entry_bytes.len() as u64;
        self.entry_count += 1;

        debug!(
            "Wrote WAL entry: shard={}, size={}",
            entry.shard_id,
            entry_bytes.len()
        );
        Ok(())
    }

    /// Check if segment needs rotation
    fn needs_rotation(&self, max_size: u64) -> bool {
        self.current_size >= max_size
    }

    /// Flush segment to disk
    fn flush(&mut self, sync_policy: SyncPolicy) -> Result<(), WALError> {
        self.file.flush()?;

        match sync_policy {
            SyncPolicy::None => {}
            SyncPolicy::Async => {
                // Update header with final entry count
                self.update_header()?;
            }
            SyncPolicy::Sync => {
                self.update_header()?;
                self.file.get_ref().sync_all()?;
            }
        }

        Ok(())
    }

    /// Update segment header with final entry count
    fn update_header(&mut self) -> Result<(), WALError> {
        // Update header
        self.header.update_entry_count(self.entry_count);
        let header_bytes = self.header.serialize()?;

        // Seek to beginning and rewrite header
        self.file.seek(SeekFrom::Start(4))?; // Skip length prefix
        self.file.write_all(&header_bytes)?;
        self.file.seek(SeekFrom::End(0))?; // Return to end

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_wal_writer_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config = WALConfig {
            wal_dir: temp_dir.path().to_path_buf(),
            max_segment_size: 1024, // Small for testing
            ..Default::default()
        };

        let writer = WALWriter::new(config).await.unwrap();

        // Write some operations
        let op1 = Operation::Put {
            key: "key1".to_string(),
            value: b"value1".to_vec(),
            ttl: None,
        };

        writer.write_operation(0, op1).await.unwrap();
        writer.flush().await.unwrap();

        // Check that segment file was created
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path()).unwrap().collect();
        assert!(!entries.is_empty());
    }
}
