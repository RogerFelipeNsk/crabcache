//! High-performance protocol parser with zero-copy optimizations

use super::commands::{Command, Response};
use bytes::Bytes;
use crate::utils::varint;
use std::str;

/// High-performance protocol parser for CrabCache binary protocol
pub struct ProtocolParser;

// Command type constants (must match serializer)
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

impl ProtocolParser {
    /// Parse a command from bytes using optimized binary format
    pub fn parse_command(bytes: &[u8]) -> crate::Result<Command> {
        if bytes.is_empty() {
            return Err("Empty command".into());
        }
        
        // Try binary format first (more efficient)
        if let Ok(cmd) = Self::parse_command_binary(bytes) {
            return Ok(cmd);
        }
        
        // Fallback to text format for backward compatibility
        Self::parse_command_text(bytes)
    }
    
    /// Parse binary format command (zero-copy when possible)
    fn parse_command_binary(bytes: &[u8]) -> crate::Result<Command> {
        if bytes.is_empty() {
            return Err("Empty binary command".into());
        }
        
        let mut cursor = 0;
        let cmd_type = bytes[cursor];
        cursor += 1;
        
        match cmd_type {
            CMD_PING => Ok(Command::Ping),
            
            CMD_PUT => {
                // Parse key
                let (key_len, key_len_bytes) = varint::decode_varint(&bytes[cursor..])?;
                cursor += key_len_bytes;
                
                if cursor + key_len as usize > bytes.len() {
                    return Err("Invalid key length in PUT command".into());
                }
                
                let key = Bytes::copy_from_slice(&bytes[cursor..cursor + key_len as usize]);
                cursor += key_len as usize;
                
                // Parse value
                let (value_len, value_len_bytes) = varint::decode_varint(&bytes[cursor..])?;
                cursor += value_len_bytes;
                
                if cursor + value_len as usize > bytes.len() {
                    return Err("Invalid value length in PUT command".into());
                }
                
                let value = Bytes::copy_from_slice(&bytes[cursor..cursor + value_len as usize]);
                cursor += value_len as usize;
                
                // Parse TTL flag
                if cursor >= bytes.len() {
                    return Err("Missing TTL flag in PUT command".into());
                }
                
                let ttl = if bytes[cursor] == 1 {
                    cursor += 1;
                    if cursor + 8 > bytes.len() {
                        return Err("Invalid TTL in PUT command".into());
                    }
                    let ttl_bytes = &bytes[cursor..cursor + 8];
                    Some(u64::from_le_bytes(ttl_bytes.try_into().unwrap()))
                } else {
                    None
                };
                
                Ok(Command::Put { key, value, ttl })
            }
            
            CMD_GET => {
                let (key_len, key_len_bytes) = varint::decode_varint(&bytes[cursor..])?;
                cursor += key_len_bytes;
                
                if cursor + key_len as usize > bytes.len() {
                    return Err("Invalid key length in GET command".into());
                }
                
                let key = Bytes::copy_from_slice(&bytes[cursor..cursor + key_len as usize]);
                Ok(Command::Get { key })
            }
            
            CMD_DEL => {
                let (key_len, key_len_bytes) = varint::decode_varint(&bytes[cursor..])?;
                cursor += key_len_bytes;
                
                if cursor + key_len as usize > bytes.len() {
                    return Err("Invalid key length in DEL command".into());
                }
                
                let key = Bytes::copy_from_slice(&bytes[cursor..cursor + key_len as usize]);
                Ok(Command::Del { key })
            }
            
            CMD_EXPIRE => {
                let (key_len, key_len_bytes) = varint::decode_varint(&bytes[cursor..])?;
                cursor += key_len_bytes;
                
                if cursor + key_len as usize + 8 > bytes.len() {
                    return Err("Invalid EXPIRE command format".into());
                }
                
                let key = Bytes::copy_from_slice(&bytes[cursor..cursor + key_len as usize]);
                cursor += key_len as usize;
                
                let ttl_bytes = &bytes[cursor..cursor + 8];
                let ttl = u64::from_le_bytes(ttl_bytes.try_into().unwrap());
                
                Ok(Command::Expire { key, ttl })
            }
            
            CMD_STATS => Ok(Command::Stats),
            CMD_METRICS => Ok(Command::Metrics),
            
            _ => Err(format!("Unknown binary command type: {}", cmd_type).into()),
        }
    }
    
    /// Parse text format command (legacy support)
    fn parse_command_text(bytes: &[u8]) -> crate::Result<Command> {
        // Convert bytes to string for parsing
        let input = str::from_utf8(bytes)?.trim();
        
        // Split command into parts
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return Err("Empty command".into());
        }
        
        let cmd = parts[0].to_uppercase();
        
        match cmd.as_str() {
            "PING" => Ok(Command::Ping),
            "PUT" => {
                if parts.len() < 3 {
                    return Err("PUT requires key and value".into());
                }
                let key = Bytes::from(parts[1].to_string());
                let value = Bytes::from(parts[2].to_string());
                let ttl = if parts.len() > 3 {
                    Some(parts[3].parse::<u64>()?)
                } else {
                    None
                };
                Ok(Command::Put { key, value, ttl })
            }
            "GET" => {
                if parts.len() < 2 {
                    return Err("GET requires key".into());
                }
                let key = Bytes::from(parts[1].to_string());
                Ok(Command::Get { key })
            }
            "DEL" => {
                if parts.len() < 2 {
                    return Err("DEL requires key".into());
                }
                let key = Bytes::from(parts[1].to_string());
                Ok(Command::Del { key })
            }
            "EXPIRE" => {
                if parts.len() < 3 {
                    return Err("EXPIRE requires key and ttl".into());
                }
                let key = Bytes::from(parts[1].to_string());
                let ttl = parts[2].parse::<u64>()?;
                Ok(Command::Expire { key, ttl })
            }
            "STATS" => Ok(Command::Stats),
            "METRICS" => Ok(Command::Metrics),
            _ => Err(format!("Unknown command: {}", cmd).into()),
        }
    }
    
    /// Parse a response from bytes using optimized binary format
    pub fn parse_response(bytes: &[u8]) -> crate::Result<Response> {
        if bytes.is_empty() {
            return Err("Empty response".into());
        }
        
        // Try binary format first
        if let Ok(resp) = Self::parse_response_binary(bytes) {
            return Ok(resp);
        }
        
        // Fallback to text format
        Self::parse_response_text(bytes)
    }
    
    /// Parse binary format response
    fn parse_response_binary(bytes: &[u8]) -> crate::Result<Response> {
        if bytes.is_empty() {
            return Err("Empty binary response".into());
        }
        
        let mut cursor = 0;
        let resp_type = bytes[cursor];
        cursor += 1;
        
        match resp_type {
            RESP_OK => Ok(Response::Ok),
            RESP_PONG => Ok(Response::Pong),
            RESP_NULL => Ok(Response::Null),
            
            RESP_ERROR => {
                let (msg_len, msg_len_bytes) = varint::decode_varint(&bytes[cursor..])?;
                cursor += msg_len_bytes;
                
                if cursor + msg_len as usize > bytes.len() {
                    return Err("Invalid error message length".into());
                }
                
                let msg = String::from_utf8_lossy(&bytes[cursor..cursor + msg_len as usize]).to_string();
                Ok(Response::Error(msg))
            }
            
            RESP_VALUE => {
                let (value_len, value_len_bytes) = varint::decode_varint(&bytes[cursor..])?;
                cursor += value_len_bytes;
                
                if cursor + value_len as usize > bytes.len() {
                    return Err("Invalid value length".into());
                }
                
                let value = Bytes::copy_from_slice(&bytes[cursor..cursor + value_len as usize]);
                Ok(Response::Value(value))
            }
            
            RESP_STATS => {
                let (stats_len, stats_len_bytes) = varint::decode_varint(&bytes[cursor..])?;
                cursor += stats_len_bytes;
                
                if cursor + stats_len as usize > bytes.len() {
                    return Err("Invalid stats length".into());
                }
                
                let stats = String::from_utf8_lossy(&bytes[cursor..cursor + stats_len as usize]).to_string();
                Ok(Response::Stats(stats))
            }
            
            _ => Err(format!("Unknown binary response type: {}", resp_type).into()),
        }
    }
    
    /// Parse text format response (legacy support)
    fn parse_response_text(bytes: &[u8]) -> crate::Result<Response> {
        let input = str::from_utf8(bytes)?.trim();
        
        if input.starts_with("OK") {
            Ok(Response::Ok)
        } else if input.starts_with("PONG") {
            Ok(Response::Pong)
        } else if input.starts_with("NULL") {
            Ok(Response::Null)
        } else if input.starts_with("ERROR:") {
            let error_msg = input.strip_prefix("ERROR:").unwrap_or("Unknown error").trim();
            Ok(Response::Error(error_msg.to_string()))
        } else if input.starts_with("STATS:") {
            let stats_data = input.strip_prefix("STATS:").unwrap_or("").trim();
            Ok(Response::Stats(stats_data.to_string()))
        } else {
            // Assume it's a value response
            Ok(Response::Value(Bytes::from(input.to_string())))
        }
    }
}