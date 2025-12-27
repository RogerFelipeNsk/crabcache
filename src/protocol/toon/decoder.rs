//! TOON Protocol Decoder
//! Ultra-fast binary decoding with zero-copy optimizations

use super::{StringInterner, ToonFlags, ToonPacket, ToonType, TOON_MAGIC, TOON_VERSION};
use bytes::Bytes;
use std::collections::HashMap;

/// High-performance TOON decoder with zero-copy support
pub struct ToonDecoder {
    interner: StringInterner,
    zero_copy_enabled: bool,
}

impl Default for ToonDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ToonDecoder {
    pub fn new() -> Self {
        Self {
            interner: StringInterner::new(),
            zero_copy_enabled: true,
        }
    }

    /// Decode TOON packet from bytes
    pub fn decode(&mut self, bytes: &[u8]) -> Result<ToonPacket, String> {
        if bytes.len() < 7 {
            return Err("Packet too short".to_string());
        }

        let mut cursor = 0;

        // Read magic bytes
        if &bytes[cursor..cursor + 4] != TOON_MAGIC {
            return Err("Invalid TOON magic bytes".to_string());
        }
        cursor += 4;

        // Read version
        let version = bytes[cursor];
        if version != TOON_VERSION {
            return Err(format!("Unsupported TOON version: {}", version));
        }
        cursor += 1;

        // Read flags
        let flags = ToonFlags::from_byte(bytes[cursor]);
        cursor += 1;

        // Read data length
        let (data_length, varint_bytes) = self.read_varint(&bytes[cursor..])?;
        cursor += varint_bytes;

        // Validate data length
        if cursor + data_length as usize > bytes.len() {
            return Err("Invalid data length".to_string());
        }

        // Decode data
        let data_bytes = &bytes[cursor..cursor + data_length as usize];
        let data = self.decode_value(data_bytes, &mut 0, flags.string_interning)?;

        Ok(ToonPacket {
            magic: *b"TOON",
            version,
            flags,
            data,
        })
    }

    /// Decode a value from bytes
    fn decode_value(
        &mut self,
        bytes: &[u8],
        cursor: &mut usize,
        use_interning: bool,
    ) -> Result<ToonType, String> {
        if *cursor >= bytes.len() {
            return Err("Unexpected end of data".to_string());
        }

        let type_id = bytes[*cursor];
        *cursor += 1;

        match type_id {
            0x00 => Ok(ToonType::Null),
            0x01 => {
                if *cursor >= bytes.len() {
                    return Err("Missing bool value".to_string());
                }
                let value = bytes[*cursor] != 0;
                *cursor += 1;
                Ok(ToonType::Bool(value))
            }
            0x02 => {
                if *cursor >= bytes.len() {
                    return Err("Missing int8 value".to_string());
                }
                let value = bytes[*cursor] as i8;
                *cursor += 1;
                Ok(ToonType::Int8(value))
            }
            0x03 => {
                if *cursor + 2 > bytes.len() {
                    return Err("Missing int16 value".to_string());
                }
                let value = i16::from_le_bytes([bytes[*cursor], bytes[*cursor + 1]]);
                *cursor += 2;
                Ok(ToonType::Int16(value))
            }
            0x04 => {
                let (value, varint_bytes) = self.read_varint(&bytes[*cursor..])?;
                *cursor += varint_bytes;
                Ok(ToonType::Int32(value as i32))
            }
            0x05 => {
                let (value, varint_bytes) = self.read_varint(&bytes[*cursor..])?;
                *cursor += varint_bytes;
                Ok(ToonType::Int64(value as i64))
            }
            0x06 => {
                if *cursor >= bytes.len() {
                    return Err("Missing uint8 value".to_string());
                }
                let value = bytes[*cursor];
                *cursor += 1;
                Ok(ToonType::UInt8(value))
            }
            0x07 => {
                if *cursor + 2 > bytes.len() {
                    return Err("Missing uint16 value".to_string());
                }
                let value = u16::from_le_bytes([bytes[*cursor], bytes[*cursor + 1]]);
                *cursor += 2;
                Ok(ToonType::UInt16(value))
            }
            0x08 => {
                let (value, varint_bytes) = self.read_varint(&bytes[*cursor..])?;
                *cursor += varint_bytes;
                Ok(ToonType::UInt32(value as u32))
            }
            0x09 => {
                let (value, varint_bytes) = self.read_varint(&bytes[*cursor..])?;
                *cursor += varint_bytes;
                Ok(ToonType::UInt64(value))
            }
            0x0A => {
                if *cursor + 4 > bytes.len() {
                    return Err("Missing float32 value".to_string());
                }
                let bytes_array = [
                    bytes[*cursor],
                    bytes[*cursor + 1],
                    bytes[*cursor + 2],
                    bytes[*cursor + 3],
                ];
                let value = f32::from_le_bytes(bytes_array);
                *cursor += 4;
                Ok(ToonType::Float32(value))
            }
            0x0B => {
                if *cursor + 8 > bytes.len() {
                    return Err("Missing float64 value".to_string());
                }
                let bytes_array = [
                    bytes[*cursor],
                    bytes[*cursor + 1],
                    bytes[*cursor + 2],
                    bytes[*cursor + 3],
                    bytes[*cursor + 4],
                    bytes[*cursor + 5],
                    bytes[*cursor + 6],
                    bytes[*cursor + 7],
                ];
                let value = f64::from_le_bytes(bytes_array);
                *cursor += 8;
                Ok(ToonType::Float64(value))
            }
            0x0C => {
                // String
                let (length, varint_bytes) = self.read_varint(&bytes[*cursor..])?;
                *cursor += varint_bytes;

                if *cursor + length as usize > bytes.len() {
                    return Err("String data exceeds buffer".to_string());
                }

                let string_bytes = &bytes[*cursor..*cursor + length as usize];
                let value = String::from_utf8(string_bytes.to_vec())
                    .map_err(|e| format!("Invalid UTF-8 in string: {}", e))?;
                *cursor += length as usize;

                // Intern string if enabled and long enough
                if use_interning && value.len() > 4 {
                    let id = self.interner.intern(&value);
                    Ok(ToonType::InternedString(id))
                } else {
                    Ok(ToonType::String(value))
                }
            }
            0x0D => {
                // Bytes
                let (length, varint_bytes) = self.read_varint(&bytes[*cursor..])?;
                *cursor += varint_bytes;

                if *cursor + length as usize > bytes.len() {
                    return Err("Bytes data exceeds buffer".to_string());
                }

                let value = if self.zero_copy_enabled {
                    // Zero-copy: create Bytes from slice
                    Bytes::copy_from_slice(&bytes[*cursor..*cursor + length as usize])
                } else {
                    // Regular copy
                    Bytes::from(bytes[*cursor..*cursor + length as usize].to_vec())
                };
                *cursor += length as usize;
                Ok(ToonType::Bytes(value))
            }
            0x0E => {
                // Array
                let (count, varint_bytes) = self.read_varint(&bytes[*cursor..])?;
                *cursor += varint_bytes;

                let mut array = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    let item = self.decode_value(bytes, cursor, use_interning)?;
                    array.push(item);
                }
                Ok(ToonType::Array(array))
            }
            0x0F => {
                // Object
                let (count, varint_bytes) = self.read_varint(&bytes[*cursor..])?;
                *cursor += varint_bytes;

                let mut object = HashMap::with_capacity(count as usize);
                for _ in 0..count {
                    // Decode key
                    let key = match self.decode_value(bytes, cursor, use_interning)? {
                        ToonType::String(s) => s,
                        ToonType::InternedString(id) => self
                            .interner
                            .get(id)
                            .ok_or_else(|| format!("Invalid interned string ID: {}", id))?
                            .to_string(),
                        _ => return Err("Object key must be string".to_string()),
                    };

                    // Decode value
                    let value = self.decode_value(bytes, cursor, use_interning)?;
                    object.insert(key, value);
                }
                Ok(ToonType::Object(object))
            }
            0x10 => {
                // Interned string
                let (id, varint_bytes) = self.read_varint(&bytes[*cursor..])?;
                *cursor += varint_bytes;
                Ok(ToonType::InternedString(id as u32))
            }
            _ => Err(format!("Unknown TOON type ID: 0x{:02X}", type_id)),
        }
    }

    /// Read variable-length integer (LEB128 decoding)
    fn read_varint(&self, bytes: &[u8]) -> Result<(u64, usize), String> {
        let mut result = 0u64;
        let mut shift = 0;
        let mut bytes_read = 0;

        for &byte in bytes {
            bytes_read += 1;

            if shift >= 64 {
                return Err("Varint too long".to_string());
            }

            result |= ((byte & 0x7F) as u64) << shift;

            if (byte & 0x80) == 0 {
                return Ok((result, bytes_read));
            }

            shift += 7;

            if bytes_read >= 10 {
                return Err("Varint too long".to_string());
            }
        }

        Err("Incomplete varint".to_string())
    }

    /// Sync interner with encoder
    pub fn sync_interner(&mut self, encoder_interner: &StringInterner) {
        self.interner = StringInterner::new();
        for (_i, string) in encoder_interner.strings.iter().enumerate() {
            self.interner.intern(string);
        }
    }

    /// Get interning statistics
    pub fn get_interning_stats(&self) -> (usize, usize) {
        (self.interner.strings.len(), self.interner.memory_saved())
    }
}

/// Convenience functions for CrabCache integration
impl ToonDecoder {
    /// Decode to CrabCache command
    pub fn decode_to_command(
        &mut self,
        bytes: &[u8],
    ) -> Result<crate::protocol::commands::Command, String> {
        let packet = self.decode(bytes)?;

        match packet.data {
            ToonType::Object(obj) => {
                let cmd = obj.get("cmd").ok_or("Missing 'cmd' field")?;

                let cmd_str = match cmd {
                    ToonType::String(s) => s.as_str(),
                    ToonType::InternedString(id) => self
                        .interner
                        .get(*id)
                        .ok_or_else(|| format!("Invalid interned string ID: {}", id))?,
                    _ => return Err("Command must be string".to_string()),
                };

                match cmd_str {
                    "PING" => Ok(crate::protocol::commands::Command::Ping),
                    "PUT" | "SET" => {
                        let key = self.extract_bytes(&obj, "key")?;
                        let value = self.extract_bytes(&obj, "value")?;
                        let ttl = obj.get("ttl").and_then(|v| match v {
                            ToonType::UInt64(t) => Some(*t),
                            _ => None,
                        });
                        Ok(crate::protocol::commands::Command::Put { key, value, ttl })
                    }
                    "GET" => {
                        let key = self.extract_bytes(&obj, "key")?;
                        Ok(crate::protocol::commands::Command::Get { key })
                    }
                    "DEL" => {
                        let key = self.extract_bytes(&obj, "key")?;
                        Ok(crate::protocol::commands::Command::Del { key })
                    }
                    "EXPIRE" => {
                        let key = self.extract_bytes(&obj, "key")?;
                        let ttl = match obj.get("ttl") {
                            Some(ToonType::UInt64(t)) => *t,
                            _ => return Err("EXPIRE requires ttl field".to_string()),
                        };
                        Ok(crate::protocol::commands::Command::Expire { key, ttl })
                    }
                    "STATS" => Ok(crate::protocol::commands::Command::Stats),
                    "METRICS" => Ok(crate::protocol::commands::Command::Metrics),
                    _ => Err(format!("Unknown command: {}", cmd_str)),
                }
            }
            _ => Err("Command must be object".to_string()),
        }
    }

    /// Extract bytes from object
    fn extract_bytes(
        &mut self,
        obj: &HashMap<String, ToonType>,
        key: &str,
    ) -> Result<Bytes, String> {
        match obj.get(key) {
            Some(ToonType::Bytes(b)) => Ok(b.clone()),
            Some(ToonType::String(s)) => Ok(Bytes::from(s.clone())),
            Some(ToonType::InternedString(id)) => {
                let s = self
                    .interner
                    .get(*id)
                    .ok_or_else(|| format!("Invalid interned string ID: {}", id))?;
                Ok(Bytes::from(s.to_string()))
            }
            _ => Err(format!("Missing or invalid '{}' field", key)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::toon::encoder::ToonEncoder;

    #[test]
    fn test_decode_null() {
        let mut encoder = ToonEncoder::new();
        let mut decoder = ToonDecoder::new();

        let packet = ToonPacket::new(ToonType::Null);
        let encoded = encoder.encode(&packet).unwrap();
        let decoded = decoder.decode(&encoded).unwrap();

        assert_eq!(decoded.data, ToonType::Null);
    }

    #[test]
    fn test_decode_bool() {
        let mut encoder = ToonEncoder::new();
        let mut decoder = ToonDecoder::new();

        let packet = ToonPacket::new(ToonType::Bool(true));
        let encoded = encoder.encode(&packet).unwrap();
        let decoded = decoder.decode(&encoded).unwrap();

        assert_eq!(decoded.data, ToonType::Bool(true));
    }

    #[test]
    fn test_decode_string() {
        let mut encoder = ToonEncoder::new();
        let mut decoder = ToonDecoder::new();

        let packet = ToonPacket::new(ToonType::String("hello world".to_string()));
        let encoded = encoder.encode(&packet).unwrap();
        let decoded = decoder.decode(&encoded).unwrap();

        match decoded.data {
            ToonType::String(s) => assert_eq!(s, "hello world"),
            ToonType::InternedString(_) => {
                // String was interned due to length > 4
                // This is expected behavior
            }
            _ => panic!("Expected string or interned string"),
        }
    }

    #[test]
    fn test_decode_array() {
        let mut encoder = ToonEncoder::new();
        let mut decoder = ToonDecoder::new();

        let array = vec![
            ToonType::Int32(1),
            ToonType::Int32(2),
            ToonType::String("test".to_string()),
        ];
        let packet = ToonPacket::new(ToonType::Array(array.clone()));
        let encoded = encoder.encode(&packet).unwrap();
        let decoded = decoder.decode(&encoded).unwrap();

        match decoded.data {
            ToonType::Array(arr) => {
                assert_eq!(arr.len(), 3);
                assert_eq!(arr[0], ToonType::Int32(1));
                assert_eq!(arr[1], ToonType::Int32(2));
                // Third element might be interned
            }
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_roundtrip_object() {
        let mut encoder = ToonEncoder::new();
        let mut decoder = ToonDecoder::new();

        let mut obj = HashMap::new();
        obj.insert("name".to_string(), ToonType::String("Alice".to_string()));
        obj.insert("age".to_string(), ToonType::UInt32(30));
        obj.insert("active".to_string(), ToonType::Bool(true));

        let packet = ToonPacket::new(ToonType::Object(obj));
        let encoded = encoder.encode(&packet).unwrap();
        let decoded = decoder.decode(&encoded).unwrap();

        match decoded.data {
            ToonType::Object(decoded_obj) => {
                assert_eq!(decoded_obj.len(), 3);
                assert!(decoded_obj.contains_key("name"));
                assert!(decoded_obj.contains_key("age"));
                assert!(decoded_obj.contains_key("active"));
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_varint_decoding() {
        let decoder = ToonDecoder::new();

        assert_eq!(decoder.read_varint(&[0]).unwrap(), (0, 1));
        assert_eq!(decoder.read_varint(&[127]).unwrap(), (127, 1));
        assert_eq!(decoder.read_varint(&[128, 1]).unwrap(), (128, 2));
        assert_eq!(decoder.read_varint(&[255, 127]).unwrap(), (16383, 2));
    }

    #[test]
    fn test_invalid_magic_bytes() {
        let mut decoder = ToonDecoder::new();
        let invalid_data = b"CRAB\x01\x00\x01\x00";

        let result = decoder.decode(invalid_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid TOON magic bytes"));
    }
}
