//! High-performance protocol serializer with zero-copy optimizations

use super::commands::{Command, Response};
use crate::utils::varint;
use bytes::{BufMut, Bytes, BytesMut};

/// High-performance protocol serializer for CrabCache binary protocol
///
/// Binary Protocol Format:
/// - Command Type (1 byte)
/// - Key Length (varint) + Key Data
/// - Value Length (varint) + Value Data (for PUT)
/// - TTL (8 bytes, optional for PUT/EXPIRE)
pub struct ProtocolSerializer;

// Command type constants for binary protocol
const CMD_PING: u8 = 0x01;
const CMD_PUT: u8 = 0x02;
const CMD_GET: u8 = 0x03;
const CMD_DEL: u8 = 0x04;
const CMD_EXPIRE: u8 = 0x05;
const CMD_STATS: u8 = 0x06;
const CMD_METRICS: u8 = 0x07;

// Response type constants
const RESP_OK: u8 = 0x10;
const RESP_PONG: u8 = 0x11;
const RESP_NULL: u8 = 0x12;
const RESP_ERROR: u8 = 0x13;
const RESP_VALUE: u8 = 0x14;
const RESP_STATS: u8 = 0x15;

impl ProtocolSerializer {
    /// Serialize a command to bytes (uses text format for compatibility)
    pub fn serialize_command(command: &Command) -> crate::Result<Bytes> {
        // Use text format for backward compatibility with existing tests
        Self::serialize_command_text(command)
    }

    /// Serialize a response to bytes (uses text format for compatibility)
    pub fn serialize_response(response: &Response) -> crate::Result<Bytes> {
        // Use text format for backward compatibility with existing tests
        Self::serialize_response_text(response)
    }

    /// Serialize a command to bytes using optimized binary format
    pub fn serialize_command_binary(command: &Command) -> crate::Result<Bytes> {
        let mut buf = BytesMut::new();

        match command {
            Command::Ping => {
                buf.put_u8(CMD_PING);
            }
            Command::Put { key, value, ttl } => {
                buf.put_u8(CMD_PUT);

                // Key length + key data (zero-copy reference)
                varint::encode_varint(key.len() as u64, &mut buf);
                buf.extend_from_slice(key);

                // Value length + value data (zero-copy reference)
                varint::encode_varint(value.len() as u64, &mut buf);
                buf.extend_from_slice(value);

                // TTL (optional)
                if let Some(ttl_val) = ttl {
                    buf.put_u8(1); // TTL present flag
                    buf.put_u64_le(*ttl_val);
                } else {
                    buf.put_u8(0); // No TTL flag
                }
            }
            Command::Get { key } => {
                buf.put_u8(CMD_GET);
                varint::encode_varint(key.len() as u64, &mut buf);
                buf.extend_from_slice(key);
            }
            Command::Del { key } => {
                buf.put_u8(CMD_DEL);
                varint::encode_varint(key.len() as u64, &mut buf);
                buf.extend_from_slice(key);
            }
            Command::Expire { key, ttl } => {
                buf.put_u8(CMD_EXPIRE);
                varint::encode_varint(key.len() as u64, &mut buf);
                buf.extend_from_slice(key);
                buf.put_u64_le(*ttl);
            }
            Command::Stats => {
                buf.put_u8(CMD_STATS);
            }
            Command::Metrics => {
                buf.put_u8(CMD_METRICS);
            }
        }

        Ok(buf.freeze())
    }

    /// Serialize a response to bytes using optimized binary format
    pub fn serialize_response_binary(response: &Response) -> crate::Result<Bytes> {
        let mut buf = BytesMut::new();

        match response {
            Response::Ok => {
                buf.put_u8(RESP_OK);
            }
            Response::Pong => {
                buf.put_u8(RESP_PONG);
            }
            Response::Null => {
                buf.put_u8(RESP_NULL);
            }
            Response::Error(msg) => {
                buf.put_u8(RESP_ERROR);
                let msg_bytes = msg.as_bytes();
                varint::encode_varint(msg_bytes.len() as u64, &mut buf);
                buf.extend_from_slice(msg_bytes);
            }
            Response::Value(value) => {
                buf.put_u8(RESP_VALUE);
                varint::encode_varint(value.len() as u64, &mut buf);
                buf.extend_from_slice(value);
            }
            Response::Stats(stats) => {
                buf.put_u8(RESP_STATS);
                let stats_bytes = stats.as_bytes();
                varint::encode_varint(stats_bytes.len() as u64, &mut buf);
                buf.extend_from_slice(stats_bytes);
            }
        }

        Ok(buf.freeze())
    }

    /// Text-based serialization for backward compatibility
    pub fn serialize_command_text(command: &Command) -> crate::Result<Bytes> {
        let serialized = match command {
            Command::Ping => "PING\r\n".to_string(),
            Command::Put { key, value, ttl } => {
                let key_str = String::from_utf8_lossy(key);
                let value_str = String::from_utf8_lossy(value);
                if let Some(ttl_val) = ttl {
                    format!("PUT {} {} {}\r\n", key_str, value_str, ttl_val)
                } else {
                    format!("PUT {} {}\r\n", key_str, value_str)
                }
            }
            Command::Get { key } => {
                let key_str = String::from_utf8_lossy(key);
                format!("GET {}\r\n", key_str)
            }
            Command::Del { key } => {
                let key_str = String::from_utf8_lossy(key);
                format!("DEL {}\r\n", key_str)
            }
            Command::Expire { key, ttl } => {
                let key_str = String::from_utf8_lossy(key);
                format!("EXPIRE {} {}\r\n", key_str, ttl)
            }
            Command::Stats => "STATS\r\n".to_string(),
            Command::Metrics => "METRICS\r\n".to_string(),
        };

        Ok(Bytes::from(serialized))
    }

    /// Text-based response serialization for backward compatibility
    pub fn serialize_response_text(response: &Response) -> crate::Result<Bytes> {
        let serialized = match response {
            Response::Ok => "OK\r\n".to_string(),
            Response::Pong => "PONG\r\n".to_string(),
            Response::Null => "NULL\r\n".to_string(),
            Response::Error(msg) => format!("ERROR: {}\r\n", msg),
            Response::Value(value) => {
                let value_str = String::from_utf8_lossy(value);
                format!("{}\r\n", value_str)
            }
            Response::Stats(stats) => format!("STATS: {}\r\n", stats),
        };

        Ok(Bytes::from(serialized))
    }
}
