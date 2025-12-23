//! WAL entry format and serialization

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use crc32fast::Hasher;

/// WAL operation types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Operation {
    Put {
        key: String,
        value: Vec<u8>,
        ttl: Option<u64>,
    },
    Delete {
        key: String,
    },
    Expire {
        key: String,
        ttl: u64,
    },
}

/// WAL entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WALEntry {
    /// Timestamp when operation occurred
    pub timestamp: u64,
    /// Shard ID where operation should be applied
    pub shard_id: usize,
    /// The operation to perform
    pub operation: Operation,
    /// CRC32 checksum for integrity
    pub checksum: u32,
}

impl WALEntry {
    /// Create a new WAL entry
    pub fn new(shard_id: usize, operation: Operation) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let mut entry = Self {
            timestamp,
            shard_id,
            operation,
            checksum: 0,
        };
        
        // Calculate checksum
        entry.checksum = entry.calculate_checksum();
        entry
    }
    
    /// Calculate CRC32 checksum for the entry
    fn calculate_checksum(&self) -> u32 {
        let mut hasher = Hasher::new();
        
        // Hash timestamp, shard_id, and operation data
        hasher.update(&self.timestamp.to_le_bytes());
        hasher.update(&(self.shard_id as u32).to_le_bytes());
        
        match &self.operation {
            Operation::Put { key, value, ttl } => {
                hasher.update(b"PUT");
                hasher.update(key.as_bytes());
                hasher.update(value);
                if let Some(ttl) = ttl {
                    hasher.update(&ttl.to_le_bytes());
                }
            }
            Operation::Delete { key } => {
                hasher.update(b"DEL");
                hasher.update(key.as_bytes());
            }
            Operation::Expire { key, ttl } => {
                hasher.update(b"EXP");
                hasher.update(key.as_bytes());
                hasher.update(&ttl.to_le_bytes());
            }
        }
        
        hasher.finalize()
    }
    
    /// Validate entry checksum
    pub fn validate_checksum(&self) -> bool {
        let expected = self.calculate_checksum();
        self.checksum == expected
    }
    
    /// Serialize entry to bytes
    pub fn serialize(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }
    
    /// Deserialize entry from bytes
    pub fn deserialize(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
    
    /// Get entry size in bytes (for length prefixing)
    pub fn serialized_size(&self) -> Result<usize, bincode::Error> {
        Ok(self.serialize()?.len())
    }
}

/// WAL segment header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentHeader {
    /// Segment version for compatibility
    pub version: u32,
    /// Segment creation timestamp
    pub created_at: u64,
    /// Number of entries in this segment
    pub entry_count: u64,
    /// Segment checksum
    pub checksum: u32,
}

impl SegmentHeader {
    /// Current WAL format version
    pub const VERSION: u32 = 1;
    
    /// Create new segment header
    pub fn new() -> Self {
        Self {
            version: Self::VERSION,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            entry_count: 0,
            checksum: 0,
        }
    }
    
    /// Update entry count and recalculate checksum
    pub fn update_entry_count(&mut self, count: u64) {
        self.entry_count = count;
        self.checksum = self.calculate_checksum();
    }
    
    /// Calculate header checksum
    fn calculate_checksum(&self) -> u32 {
        let mut hasher = Hasher::new();
        hasher.update(&self.version.to_le_bytes());
        hasher.update(&self.created_at.to_le_bytes());
        hasher.update(&self.entry_count.to_le_bytes());
        hasher.finalize()
    }
    
    /// Validate header checksum
    pub fn validate_checksum(&self) -> bool {
        let expected = self.calculate_checksum();
        self.checksum == expected
    }
    
    /// Serialize header to bytes
    pub fn serialize(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }
    
    /// Deserialize header from bytes
    pub fn deserialize(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_entry_checksum() {
        let operation = Operation::Put {
            key: "test_key".to_string(),
            value: b"test_value".to_vec(),
            ttl: Some(3600),
        };
        
        let entry = WALEntry::new(0, operation);
        assert!(entry.validate_checksum());
        
        // Test serialization roundtrip
        let serialized = entry.serialize().unwrap();
        let deserialized = WALEntry::deserialize(&serialized).unwrap();
        assert!(deserialized.validate_checksum());
        assert_eq!(entry.operation, deserialized.operation);
    }
    
    #[test]
    fn test_segment_header() {
        let mut header = SegmentHeader::new();
        header.update_entry_count(100);
        assert!(header.validate_checksum());
        
        // Test serialization roundtrip
        let serialized = header.serialize().unwrap();
        let deserialized = SegmentHeader::deserialize(&serialized).unwrap();
        assert!(deserialized.validate_checksum());
        assert_eq!(header.entry_count, deserialized.entry_count);
    }
}