//! TOON Protocol Encoder
//! Ultra-compact binary encoding with zero-copy optimizations

use super::{StringInterner, ToonPacket, ToonType};
use bytes::{BufMut, BytesMut};
use std::collections::HashMap;

/// High-performance TOON encoder with string interning
pub struct ToonEncoder {
    interner: StringInterner,
    zero_copy_enabled: bool,
    simd_enabled: bool,
}

impl Default for ToonEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ToonEncoder {
    pub fn new() -> Self {
        Self {
            interner: StringInterner::new(),
            zero_copy_enabled: true,
            simd_enabled: cfg!(target_feature = "avx2"),
        }
    }

    /// Encode a TOON packet to bytes
    pub fn encode(&mut self, packet: &ToonPacket) -> Result<BytesMut, String> {
        let estimated_size = packet.estimated_size();
        let mut buf = BytesMut::with_capacity(estimated_size);

        // Write header
        buf.put_slice(&packet.magic);
        buf.put_u8(packet.version);
        buf.put_u8(packet.flags.to_byte());

        // Encode data and write length
        let data_start = buf.len();
        buf.put_u8(0); // Placeholder for length varint
        let length_pos = buf.len() - 1;

        self.encode_value(&packet.data, &mut buf, packet.flags.string_interning)?;

        // Update length field
        let data_length = buf.len() - data_start - 1;
        self.write_varint_at(&mut buf, length_pos, data_length as u64)?;

        Ok(buf)
    }

    /// Encode a value with optimal type selection
    fn encode_value(
        &mut self,
        value: &ToonType,
        buf: &mut BytesMut,
        use_interning: bool,
    ) -> Result<(), String> {
        buf.put_u8(value.type_id());

        match value {
            ToonType::Null => {
                // No additional data needed
            }
            ToonType::Bool(b) => {
                buf.put_u8(if *b { 1 } else { 0 });
            }
            ToonType::Int8(v) => {
                buf.put_i8(*v);
            }
            ToonType::Int16(v) => {
                buf.put_i16_le(*v);
            }
            ToonType::Int32(v) => {
                self.write_varint(buf, *v as u64);
            }
            ToonType::Int64(v) => {
                self.write_varint(buf, *v as u64);
            }
            ToonType::UInt8(v) => {
                buf.put_u8(*v);
            }
            ToonType::UInt16(v) => {
                buf.put_u16_le(*v);
            }
            ToonType::UInt32(v) => {
                self.write_varint(buf, *v as u64);
            }
            ToonType::UInt64(v) => {
                self.write_varint(buf, *v);
            }
            ToonType::Float32(v) => {
                buf.put_f32_le(*v);
            }
            ToonType::Float64(v) => {
                buf.put_f64_le(*v);
            }
            ToonType::String(s) => {
                if use_interning && s.len() > 4 {
                    // Use interning for strings longer than 4 chars
                    let id = self.interner.intern(s);
                    buf.put_u8(ToonType::InternedString(id).type_id());
                    self.write_varint(buf, id as u64);
                } else {
                    self.write_varint(buf, s.len() as u64);
                    buf.put_slice(s.as_bytes());
                }
            }
            ToonType::Bytes(b) => {
                self.write_varint(buf, b.len() as u64);
                buf.put_slice(b);
            }
            ToonType::Array(arr) => {
                self.write_varint(buf, arr.len() as u64);
                for item in arr {
                    self.encode_value(item, buf, use_interning)?;
                }
            }
            ToonType::Object(obj) => {
                self.write_varint(buf, obj.len() as u64);
                for (key, value) in obj {
                    // Encode key as string
                    if use_interning && key.len() > 4 {
                        let id = self.interner.intern(key);
                        buf.put_u8(ToonType::InternedString(id).type_id());
                        self.write_varint(buf, id as u64);
                    } else {
                        buf.put_u8(ToonType::String(key.clone()).type_id());
                        self.write_varint(buf, key.len() as u64);
                        buf.put_slice(key.as_bytes());
                    }
                    // Encode value
                    self.encode_value(value, buf, use_interning)?;
                }
            }
            ToonType::InternedString(id) => {
                self.write_varint(buf, *id as u64);
            }
        }

        Ok(())
    }

    /// Write variable-length integer (LEB128 encoding)
    fn write_varint(&self, buf: &mut BytesMut, mut value: u64) {
        while value >= 0x80 {
            buf.put_u8((value & 0x7F) as u8 | 0x80);
            value >>= 7;
        }
        buf.put_u8(value as u8);
    }

    /// Write varint at specific position (for length field)
    fn write_varint_at(&self, buf: &mut BytesMut, pos: usize, value: u64) -> Result<(), String> {
        let varint_bytes = self.encode_varint_bytes(value);

        if varint_bytes.len() == 1 {
            // Perfect fit
            buf[pos] = varint_bytes[0];
            Ok(())
        } else {
            // Need to shift data to make room
            let shift_amount = varint_bytes.len() - 1;
            let old_len = buf.len();
            buf.resize(old_len + shift_amount, 0);

            // Shift data right
            for i in (pos + 1..old_len).rev() {
                buf[i + shift_amount] = buf[i];
            }

            // Write varint
            for (i, &byte) in varint_bytes.iter().enumerate() {
                buf[pos + i] = byte;
            }

            Ok(())
        }
    }

    /// Encode varint to bytes
    fn encode_varint_bytes(&self, mut value: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        while value >= 0x80 {
            bytes.push((value & 0x7F) as u8 | 0x80);
            value >>= 7;
        }
        bytes.push(value as u8);
        bytes
    }

    /// Get interning statistics
    pub fn get_interning_stats(&self) -> (usize, usize) {
        (self.interner.strings.len(), self.interner.memory_saved())
    }

    /// Reset interner (for new session)
    pub fn reset_interner(&mut self) {
        self.interner = StringInterner::new();
    }
}

/// Convenience functions for common encoding tasks
impl ToonEncoder {
    /// Encode a simple key-value pair
    pub fn encode_kv(&mut self, key: &str, value: ToonType) -> Result<BytesMut, String> {
        let mut obj = HashMap::new();
        obj.insert(key.to_string(), value);

        let packet = ToonPacket::new(ToonType::Object(obj));
        self.encode(&packet)
    }

    /// Encode CrabCache command response
    pub fn encode_response(
        &mut self,
        response: &crate::protocol::commands::Response,
    ) -> Result<BytesMut, String> {
        let toon_value = match response {
            crate::protocol::commands::Response::Ok => ToonType::String("OK".to_string()),
            crate::protocol::commands::Response::Pong => ToonType::String("PONG".to_string()),
            crate::protocol::commands::Response::Null => ToonType::Null,
            crate::protocol::commands::Response::Error(msg) => {
                let mut obj = HashMap::new();
                obj.insert("error".to_string(), ToonType::String(msg.clone()));
                ToonType::Object(obj)
            }
            crate::protocol::commands::Response::Value(bytes) => ToonType::Bytes(bytes.clone()),
            crate::protocol::commands::Response::Stats(stats) => ToonType::String(stats.clone()),
        };

        let packet = ToonPacket::new(toon_value);
        self.encode(&packet)
    }

    /// Encode CrabCache command
    pub fn encode_command(
        &mut self,
        command: &crate::protocol::commands::Command,
    ) -> Result<BytesMut, String> {
        let toon_value = match command {
            crate::protocol::commands::Command::Ping => {
                let mut obj = HashMap::new();
                obj.insert("cmd".to_string(), ToonType::String("PING".to_string()));
                ToonType::Object(obj)
            }
            crate::protocol::commands::Command::Put { key, value, ttl } => {
                let mut obj = HashMap::new();
                obj.insert("cmd".to_string(), ToonType::String("PUT".to_string()));
                obj.insert("key".to_string(), ToonType::Bytes(key.clone()));
                obj.insert("value".to_string(), ToonType::Bytes(value.clone()));
                if let Some(ttl_val) = ttl {
                    obj.insert("ttl".to_string(), ToonType::UInt64(*ttl_val));
                }
                ToonType::Object(obj)
            }
            crate::protocol::commands::Command::Get { key } => {
                let mut obj = HashMap::new();
                obj.insert("cmd".to_string(), ToonType::String("GET".to_string()));
                obj.insert("key".to_string(), ToonType::Bytes(key.clone()));
                ToonType::Object(obj)
            }
            crate::protocol::commands::Command::Del { key } => {
                let mut obj = HashMap::new();
                obj.insert("cmd".to_string(), ToonType::String("DEL".to_string()));
                obj.insert("key".to_string(), ToonType::Bytes(key.clone()));
                ToonType::Object(obj)
            }
            crate::protocol::commands::Command::Expire { key, ttl } => {
                let mut obj = HashMap::new();
                obj.insert("cmd".to_string(), ToonType::String("EXPIRE".to_string()));
                obj.insert("key".to_string(), ToonType::Bytes(key.clone()));
                obj.insert("ttl".to_string(), ToonType::UInt64(*ttl));
                ToonType::Object(obj)
            }
            crate::protocol::commands::Command::Stats => {
                let mut obj = HashMap::new();
                obj.insert("cmd".to_string(), ToonType::String("STATS".to_string()));
                ToonType::Object(obj)
            }
            crate::protocol::commands::Command::Metrics => {
                let mut obj = HashMap::new();
                obj.insert("cmd".to_string(), ToonType::String("METRICS".to_string()));
                ToonType::Object(obj)
            }
        };

        let packet = ToonPacket::new(toon_value);
        self.encode(&packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_null() {
        let mut encoder = ToonEncoder::new();
        let packet = ToonPacket::new(ToonType::Null);
        let result = encoder.encode(&packet).unwrap();

        // Should be: TOON + version + flags + length + type
        assert!(result.len() >= 7); // Minimum size
        assert_eq!(&result[0..4], b"TOON");
    }

    #[test]
    fn test_encode_bool() {
        let mut encoder = ToonEncoder::new();
        let packet = ToonPacket::new(ToonType::Bool(true));
        let result = encoder.encode(&packet).unwrap();

        assert_eq!(&result[0..4], b"TOON");
        // Should contain bool type and value
        assert!(result.len() >= 8);
    }

    #[test]
    fn test_encode_string() {
        let mut encoder = ToonEncoder::new();
        let packet = ToonPacket::new(ToonType::String("hello".to_string()));
        let result = encoder.encode(&packet).unwrap();

        assert_eq!(&result[0..4], b"TOON");
        // Should contain string data
        assert!(result.len() >= 12); // header + type + length + "hello"
    }

    #[test]
    fn test_string_interning() {
        let mut encoder = ToonEncoder::new();

        // First occurrence - should be stored normally
        let packet1 = ToonPacket::new(ToonType::String("repeated_string".to_string()));
        let _result1 = encoder.encode(&packet1).unwrap();

        // Second occurrence - should use interning
        let packet2 = ToonPacket::new(ToonType::String("repeated_string".to_string()));
        let _result2 = encoder.encode(&packet2).unwrap();

        // Second encoding should be smaller due to interning
        // (This is a simplified test - in practice the difference would be more significant)
        let (interned_count, _memory_saved) = encoder.get_interning_stats();
        assert!(interned_count > 0);
    }

    #[test]
    fn test_encode_array() {
        let mut encoder = ToonEncoder::new();
        let array = vec![ToonType::Int32(1), ToonType::Int32(2), ToonType::Int32(3)];
        let packet = ToonPacket::new(ToonType::Array(array));
        let result = encoder.encode(&packet).unwrap();

        assert_eq!(&result[0..4], b"TOON");
        assert!(result.len() >= 10); // Should contain array data
    }

    #[test]
    fn test_encode_object() {
        let mut encoder = ToonEncoder::new();
        let mut obj = HashMap::new();
        obj.insert("key1".to_string(), ToonType::String("value1".to_string()));
        obj.insert("key2".to_string(), ToonType::Int32(42));

        let packet = ToonPacket::new(ToonType::Object(obj));
        let result = encoder.encode(&packet).unwrap();

        assert_eq!(&result[0..4], b"TOON");
        assert!(result.len() >= 15); // Should contain object data
    }

    #[test]
    fn test_varint_encoding() {
        let encoder = ToonEncoder::new();

        assert_eq!(encoder.encode_varint_bytes(0), vec![0]);
        assert_eq!(encoder.encode_varint_bytes(127), vec![127]);
        assert_eq!(encoder.encode_varint_bytes(128), vec![128, 1]);
        assert_eq!(encoder.encode_varint_bytes(16383), vec![255, 127]);
        assert_eq!(encoder.encode_varint_bytes(16384), vec![128, 128, 1]);
    }
}
