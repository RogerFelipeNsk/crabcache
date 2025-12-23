//! Cache item implementation with binary layout

use bytes::{Bytes, BytesMut, Buf, BufMut};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::utils::varint::{encode_varint, decode_varint, varint_size};

/// A cache item with binary layout
/// 
/// Binary format:
/// | key_len (varint) |
/// | key bytes        |
/// | value_len(varint)|
/// | value bytes      |
/// | expires_at(u64)  |
/// | flags(u8)        |
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub key: Bytes,
    pub value: Bytes,
    pub expires_at: Option<u64>,
    pub flags: u8,
}

impl Item {
    /// Create a new item
    pub fn new(key: Bytes, value: Bytes, expires_at: Option<u64>) -> Self {
        Self {
            key,
            value,
            expires_at,
            flags: 0,
        }
    }
    
    /// Create a new item with TTL in seconds
    pub fn with_ttl(key: Bytes, value: Bytes, ttl_seconds: u64) -> Self {
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() + ttl_seconds;
        
        Self {
            key,
            value,
            expires_at: Some(expires_at),
            flags: 0,
        }
    }
    
    /// Check if item is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now >= expires_at
        } else {
            false
        }
    }
    
    /// Calculate the binary size of this item
    pub fn binary_size(&self) -> usize {
        varint_size(self.key.len() as u64) +
        self.key.len() +
        varint_size(self.value.len() as u64) +
        self.value.len() +
        8 + // expires_at (u64)
        1   // flags (u8)
    }
    
    /// Serialize item to binary format
    pub fn to_binary(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(self.binary_size());
        
        // key_len (varint)
        encode_varint(self.key.len() as u64, &mut buf);
        
        // key bytes
        buf.put_slice(&self.key);
        
        // value_len (varint)
        encode_varint(self.value.len() as u64, &mut buf);
        
        // value bytes
        buf.put_slice(&self.value);
        
        // expires_at (u64, little-endian)
        buf.put_u64_le(self.expires_at.unwrap_or(0));
        
        // flags (u8)
        buf.put_u8(self.flags);
        
        buf.freeze()
    }
    
    /// Deserialize item from binary format
    pub fn from_binary(mut data: Bytes) -> Result<Self, String> {
        if data.is_empty() {
            return Err("Empty data".to_string());
        }
        
        let mut cursor = 0;
        
        // key_len (varint)
        let (key_len, key_len_bytes) = decode_varint(&data[cursor..])
            .map_err(|e| format!("Failed to decode key length: {}", e))?;
        cursor += key_len_bytes;
        
        if cursor + key_len as usize > data.len() {
            return Err("Insufficient data for key".to_string());
        }
        
        // key bytes
        let key = data.slice(cursor..cursor + key_len as usize);
        cursor += key_len as usize;
        
        // value_len (varint)
        let (value_len, value_len_bytes) = decode_varint(&data[cursor..])
            .map_err(|e| format!("Failed to decode value length: {}", e))?;
        cursor += value_len_bytes;
        
        if cursor + value_len as usize > data.len() {
            return Err("Insufficient data for value".to_string());
        }
        
        // value bytes
        let value = data.slice(cursor..cursor + value_len as usize);
        cursor += value_len as usize;
        
        if cursor + 8 > data.len() {
            return Err("Insufficient data for expires_at".to_string());
        }
        
        // expires_at (u64, little-endian)
        let expires_at_raw = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .map_err(|_| "Failed to read expires_at")?
        );
        cursor += 8;
        
        let expires_at = if expires_at_raw == 0 {
            None
        } else {
            Some(expires_at_raw)
        };
        
        // flags (u8)
        if cursor >= data.len() {
            return Err("Insufficient data for flags".to_string());
        }
        let flags = data[cursor];
        
        Ok(Item {
            key,
            value,
            expires_at,
            flags,
        })
    }
}