//! WAL reader implementation for recovery

use crate::wal::entry::{WALEntry, SegmentHeader, Operation};
use crate::wal::writer::WALError;
use std::fs::File;
use std::io::{Read, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tracing::{info, warn, error, debug};

/// WAL reader for recovery operations
pub struct WALReader {
    wal_dir: PathBuf,
}

/// Recovery statistics
#[derive(Debug, Default)]
pub struct RecoveryStats {
    pub segments_processed: usize,
    pub entries_recovered: usize,
    pub entries_skipped: usize,
    pub corrupted_entries: usize,
    pub recovery_time_ms: u64,
}

impl WALReader {
    /// Create new WAL reader
    pub fn new<P: AsRef<Path>>(wal_dir: P) -> Self {
        Self {
            wal_dir: wal_dir.as_ref().to_path_buf(),
        }
    }

    /// Recover all operations from WAL segments
    pub async fn recover_all(&self) -> Result<(Vec<WALEntry>, RecoveryStats), WALError> {
        let start_time = std::time::Instant::now();
        let mut stats = RecoveryStats::default();
        let mut all_entries = Vec::new();

        info!("Starting WAL recovery from: {:?}", self.wal_dir);

        if !self.wal_dir.exists() {
            info!("WAL directory does not exist, no recovery needed");
            return Ok((all_entries, stats));
        }

        // Get all WAL segment files
        let segments = self.get_segment_files()?;
        stats.segments_processed = segments.len();

        if segments.is_empty() {
            info!("No WAL segments found, no recovery needed");
            return Ok((all_entries, stats));
        }

        info!("Found {} WAL segments to process", segments.len());

        // Process segments in chronological order
        for segment_path in segments {
            match self.read_segment(&segment_path).await {
                Ok(entries) => {
                    let entries_len = entries.len();
                    stats.entries_recovered += entries_len;
                    all_entries.extend(entries);
                    debug!("Recovered {} entries from {:?}", entries_len, segment_path);
                }
                Err(e) => {
                    warn!("Failed to read segment {:?}: {}", segment_path, e);
                    stats.entries_skipped += 1;
                }
            }
        }

        stats.recovery_time_ms = start_time.elapsed().as_millis() as u64;
        
        info!(
            "WAL recovery completed: {} entries from {} segments in {}ms",
            stats.entries_recovered,
            stats.segments_processed,
            stats.recovery_time_ms
        );

        Ok((all_entries, stats))
    }

    /// Read entries from a specific segment
    pub async fn read_segment(&self, segment_path: &Path) -> Result<Vec<WALEntry>, WALError> {
        let file = File::open(segment_path)?;
        let mut reader = BufReader::new(file);
        let mut entries = Vec::new();

        debug!("Reading WAL segment: {:?}", segment_path);

        // Read and validate header
        let header = self.read_segment_header(&mut reader)?;
        if !header.validate_checksum() {
            warn!("Invalid header checksum in segment: {:?}", segment_path);
            return Err(WALError::Serialization(bincode::Error::new(
                bincode::ErrorKind::Custom("Invalid header checksum".to_string())
            )));
        }

        debug!("Segment header: {} entries, created at {}", 
               header.entry_count, header.created_at);

        // Read entries
        let mut entries_read = 0;
        while entries_read < header.entry_count {
            match self.read_next_entry(&mut reader) {
                Ok(Some(entry)) => {
                    if entry.validate_checksum() {
                        entries.push(entry);
                        entries_read += 1;
                    } else {
                        warn!("Corrupted entry checksum in segment: {:?}", segment_path);
                        break;
                    }
                }
                Ok(None) => {
                    // End of file reached
                    break;
                }
                Err(e) => {
                    warn!("Error reading entry from segment {:?}: {}", segment_path, e);
                    break;
                }
            }
        }

        if entries_read as u64 != header.entry_count {
            warn!(
                "Expected {} entries but read {} from segment: {:?}",
                header.entry_count, entries_read, segment_path
            );
        }

        Ok(entries)
    }

    /// Get all segment files sorted by creation time
    fn get_segment_files(&self) -> Result<Vec<PathBuf>, WALError> {
        let mut segments = Vec::new();

        for entry in std::fs::read_dir(&self.wal_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() && 
               path.extension().map_or(false, |ext| ext == "log") &&
               path.file_name()
                   .and_then(|name| name.to_str())
                   .map_or(false, |name| name.starts_with("wal-")) {
                segments.push(path);
            }
        }

        // Sort by filename (which contains timestamp)
        segments.sort();
        Ok(segments)
    }

    /// Read segment header
    fn read_segment_header(&self, reader: &mut BufReader<File>) -> Result<SegmentHeader, WALError> {
        // Read header length
        let mut len_bytes = [0u8; 4];
        reader.read_exact(&mut len_bytes)?;
        let header_len = u32::from_le_bytes(len_bytes) as usize;

        // Read header data
        let mut header_bytes = vec![0u8; header_len];
        reader.read_exact(&mut header_bytes)?;

        // Deserialize header
        let header = SegmentHeader::deserialize(&header_bytes)?;
        Ok(header)
    }

    /// Read next entry from segment
    fn read_next_entry(&self, reader: &mut BufReader<File>) -> Result<Option<WALEntry>, WALError> {
        // Try to read entry length
        let mut len_bytes = [0u8; 4];
        match reader.read_exact(&mut len_bytes) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None); // End of file
            }
            Err(e) => return Err(WALError::Io(e)),
        }

        let entry_len = u32::from_le_bytes(len_bytes) as usize;
        
        // Sanity check entry length
        if entry_len > 10 * 1024 * 1024 { // 10MB max entry size
            return Err(WALError::Serialization(bincode::Error::new(
                bincode::ErrorKind::Custom("Entry too large".to_string())
            )));
        }

        // Read entry data
        let mut entry_bytes = vec![0u8; entry_len];
        reader.read_exact(&mut entry_bytes)?;

        // Deserialize entry
        let entry = WALEntry::deserialize(&entry_bytes)?;
        Ok(Some(entry))
    }

    /// Replay operations to a shard manager
    pub async fn replay_to_manager<M>(&self, manager: &M) -> Result<RecoveryStats, WALError>
    where
        M: WALReplayTarget,
    {
        let (entries, mut stats) = self.recover_all().await?;
        
        info!("Replaying {} operations to shard manager", entries.len());
        
        for entry in entries {
            match manager.replay_operation(entry.shard_id, &entry.operation).await {
                Ok(()) => {
                    debug!("Replayed operation: shard={}, op={:?}", 
                           entry.shard_id, entry.operation);
                }
                Err(e) => {
                    warn!("Failed to replay operation: {}", e);
                    stats.entries_skipped += 1;
                }
            }
        }

        info!("WAL replay completed: {} operations replayed", 
              stats.entries_recovered - stats.entries_skipped);
        
        Ok(stats)
    }

    /// Clean up old WAL segments (for maintenance)
    pub async fn cleanup_old_segments(&self, keep_segments: usize) -> Result<usize, WALError> {
        let mut segments = self.get_segment_files()?;
        
        if segments.len() <= keep_segments {
            return Ok(0);
        }

        // Sort by creation time (oldest first)
        segments.sort();
        
        let to_remove = segments.len() - keep_segments;
        let mut removed = 0;

        for segment in segments.iter().take(to_remove) {
            match std::fs::remove_file(segment) {
                Ok(()) => {
                    info!("Removed old WAL segment: {:?}", segment);
                    removed += 1;
                }
                Err(e) => {
                    warn!("Failed to remove WAL segment {:?}: {}", segment, e);
                }
            }
        }

        Ok(removed)
    }
}

/// Trait for types that can replay WAL operations
#[async_trait::async_trait]
pub trait WALReplayTarget {
    type Error: std::fmt::Display;
    
    /// Replay a single operation to the specified shard
    async fn replay_operation(&self, shard_id: usize, operation: &Operation) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::writer::{WALWriter, WALConfig};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_wal_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let wal_dir = temp_dir.path().to_path_buf();
        
        // Write some operations
        {
            let config = WALConfig {
                wal_dir: wal_dir.clone(),
                max_segment_size: 1024,
                ..Default::default()
            };
            
            let writer = WALWriter::new(config).await.unwrap();
            
            let op1 = Operation::Put {
                key: "key1".to_string(),
                value: b"value1".to_vec(),
                ttl: None,
            };
            
            let op2 = Operation::Delete {
                key: "key2".to_string(),
            };
            
            writer.write_operation(0, op1).await.unwrap();
            writer.write_operation(1, op2).await.unwrap();
            writer.flush().await.unwrap();
        }
        
        // Read them back
        let reader = WALReader::new(&wal_dir);
        let (entries, stats) = reader.recover_all().await.unwrap();
        
        assert_eq!(entries.len(), 2);
        assert_eq!(stats.entries_recovered, 2);
        assert_eq!(stats.segments_processed, 1);
        
        // Verify operations
        match &entries[0].operation {
            Operation::Put { key, value, .. } => {
                assert_eq!(key, "key1");
                assert_eq!(value, b"value1");
            }
            _ => panic!("Expected Put operation"),
        }
        
        match &entries[1].operation {
            Operation::Delete { key } => {
                assert_eq!(key, "key2");
            }
            _ => panic!("Expected Delete operation"),
        }
    }
}