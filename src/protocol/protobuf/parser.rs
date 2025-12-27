//! Protobuf Parser for CrabCache
//! High-performance parser with zero-copy optimizations

use bytes::{Bytes, Buf};
use std::time::Instant;

use crate::protocol::Command;
use crate::protocol::protobuf::{
    ProtobufError, ProtobufResult, ProtobufConfig, ProtobufMetrics,
    generated::{crab_cache_command, CrabCacheCommand},
};

/// High-performance Protobuf parser
pub struct ProtobufParser {
    config: ProtobufConfig,
    metrics: ProtobufMetrics,
    zero_copy_enabled: bool,
}

impl ProtobufParser {
    pub fn new(config: ProtobufConfig) -> Self {
        Self {
            zero_copy_enabled: config.enable_zero_copy,
            config,
            metrics: ProtobufMetrics::default(),
        }
    }
    
    /// Parse a Protobuf message into CrabCache command
    pub fn parse_command(&mut self, data: Bytes) -> ProtobufResult<Command> {
        let start_time = Instant::now();
        let data_len = data.len();
        
        // Validate message size
        if data_len > self.config.max_message_size {
            return Err(ProtobufError::MessageTooLarge {
                size: data_len,
                max: self.config.max_message_size,
            });
        }
        
        let mut data = data;
        
        // Skip magic bytes and version if present
        if data.len() >= 6 && data[0..4] == crate::protocol::protobuf::PROTOBUF_MAGIC {
            data.advance(6); // Skip magic + version + length
        }
        
        // Parse the protobuf message
        let proto_command = CrabCacheCommand::decode(&data)
            .map_err(|e| ProtobufError::DecodeError(prost::DecodeError::new("Stub decode error")))?;
        
        // Convert to internal command format
        let command = self.convert_proto_to_command(proto_command)?;
        
        // Update metrics
        let parse_time = start_time.elapsed().as_micros() as f64;
        self.metrics.update_message_processed(data_len, parse_time, 0.0);
        
        Ok(command)
    }
    
    /// Parse multiple commands from a batch
    pub fn parse_batch(&mut self, data: Bytes) -> ProtobufResult<Vec<Command>> {
        let start_time = Instant::now();
        
        // Parse batch command
        let proto_command = CrabCacheCommand::decode(&data)
            .map_err(|e| ProtobufError::DecodeError(prost::DecodeError::new("Stub decode error")))?;
        
        match proto_command.command {
            Some(crab_cache_command::Command::Batch(batch_cmd)) => {
                let mut commands = Vec::with_capacity(batch_cmd.commands.len());
                
                for proto_cmd in batch_cmd.commands {
                    let command = self.convert_proto_to_command(proto_cmd)?;
                    commands.push(command);
                }
                
                // Update metrics
                let parse_time = start_time.elapsed().as_micros() as f64;
                self.metrics.update_message_processed(data.len(), parse_time, 0.0);
                
                Ok(commands)
            }
            _ => {
                // Single command, wrap in vec
                let command = self.convert_proto_to_command(proto_command)?;
                Ok(vec![command])
            }
        }
    }
    
    /// Convert protobuf command to internal command
    fn convert_proto_to_command(&self, proto_cmd: CrabCacheCommand) -> ProtobufResult<Command> {
        match proto_cmd.command {
            Some(crab_cache_command::Command::Put(put_cmd)) => {
                Ok(Command::Put {
                    key: put_cmd.key,
                    value: put_cmd.value,
                    ttl: put_cmd.ttl_seconds,
                })
            }
            
            Some(crab_cache_command::Command::Get(get_cmd)) => {
                Ok(Command::Get {
                    key: get_cmd.key,
                })
            }
            
            Some(crab_cache_command::Command::Del(del_cmd)) => {
                Ok(Command::Del {
                    key: del_cmd.key,
                })
            }
            
            Some(crab_cache_command::Command::Expire(expire_cmd)) => {
                Ok(Command::Expire {
                    key: expire_cmd.key,
                    ttl: expire_cmd.ttl_seconds,
                })
            }
            
            Some(crab_cache_command::Command::Stats(_)) => Ok(Command::Stats),
            Some(crab_cache_command::Command::Metrics(_)) => Ok(Command::Metrics),
            Some(crab_cache_command::Command::Ping(_)) => Ok(Command::Ping),
            
            Some(crab_cache_command::Command::Batch(_)) => {
                Err(ProtobufError::DecodeError(
                    prost::DecodeError::new("Batch commands should be handled separately")
                ))
            }
            
            None => {
                Err(ProtobufError::DecodeError(
                    prost::DecodeError::new("No command specified")
                ))
            }
        }
    }
    
    /// Get current metrics
    pub fn get_metrics(&self) -> &ProtobufMetrics {
        &self.metrics
    }
    
    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = ProtobufMetrics::default();
    }
    
    /// Check if zero-copy is enabled
    pub fn is_zero_copy_enabled(&self) -> bool {
        self.zero_copy_enabled
    }
    
    /// Enable/disable zero-copy optimizations
    pub fn set_zero_copy_enabled(&mut self, enabled: bool) {
        self.zero_copy_enabled = enabled;
    }
}

impl Default for ProtobufParser {
    fn default() -> Self {
        Self::new(ProtobufConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    
    #[test]
    fn test_parse_put_command() {
        let mut parser = ProtobufParser::default();
        
        // Create a PUT command
        let put_cmd = PutCommand {
            key: b"test_key".to_vec(),
            value: b"test_value".to_vec(),
            ttl_seconds: Some(3600),
            metadata: std::collections::HashMap::new(),
        };
        
        let proto_cmd = CrabCacheCommand {
            request_id: "test_123".to_string(),
            timestamp: 1234567890,
            command: Some(CommandType::Put(put_cmd)),
        };
        
        // Encode to bytes
        let mut buf = Vec::new();
        proto_cmd.encode(&mut buf).unwrap();
        let data = Bytes::from(buf);
        
        // Parse back
        let parsed_cmd = parser.parse_command(data).unwrap();
        
        match parsed_cmd {
            Command::Put { key, value, ttl } => {
                assert_eq!(key, Bytes::from("test_key"));
                assert_eq!(value, Bytes::from("test_value"));
                assert_eq!(ttl, Some(3600));
            }
            _ => panic!("Expected PUT command"),
        }
    }
    
    #[test]
    fn test_parse_get_command() {
        let mut parser = ProtobufParser::default();
        
        let get_cmd = GetCommand {
            key: b"test_key".to_vec(),
            include_metadata: false,
        };
        
        let proto_cmd = CrabCacheCommand {
            request_id: "test_456".to_string(),
            timestamp: 1234567890,
            command: Some(CommandType::Get(get_cmd)),
        };
        
        let mut buf = Vec::new();
        proto_cmd.encode(&mut buf).unwrap();
        let data = Bytes::from(buf);
        
        let parsed_cmd = parser.parse_command(data).unwrap();
        
        match parsed_cmd {
            Command::Get { key } => {
                assert_eq!(key, Bytes::from("test_key"));
            }
            _ => panic!("Expected GET command"),
        }
    }
    
    #[test]
    fn test_parse_batch_commands() {
        let mut parser = ProtobufParser::default();
        
        // Create individual commands
        let put_cmd = CrabCacheCommand {
            request_id: "batch_1".to_string(),
            timestamp: 1234567890,
            command: Some(CommandType::Put(PutCommand {
                key: b"key1".to_vec(),
                value: b"value1".to_vec(),
                ttl_seconds: None,
                metadata: std::collections::HashMap::new(),
            })),
        };
        
        let get_cmd = CrabCacheCommand {
            request_id: "batch_2".to_string(),
            timestamp: 1234567890,
            command: Some(CommandType::Get(GetCommand {
                key: b"key2".to_vec(),
                include_metadata: false,
            })),
        };
        
        // Create batch command
        let batch_cmd = BatchCommand {
            commands: vec![put_cmd, get_cmd],
            atomic: false,
        };
        
        let proto_cmd = CrabCacheCommand {
            request_id: "batch_main".to_string(),
            timestamp: 1234567890,
            command: Some(CommandType::Batch(batch_cmd)),
        };
        
        let mut buf = Vec::new();
        proto_cmd.encode(&mut buf).unwrap();
        let data = Bytes::from(buf);
        
        let parsed_commands = parser.parse_batch(data).unwrap();
        
        assert_eq!(parsed_commands.len(), 2);
        
        match &parsed_commands[0] {
            Command::Put { key, value, .. } => {
                assert_eq!(key, &Bytes::from("key1"));
                assert_eq!(value, &Bytes::from("value1"));
            }
            _ => panic!("Expected PUT command"),
        }
        
        match &parsed_commands[1] {
            Command::Get { key } => {
                assert_eq!(key, &Bytes::from("key2"));
            }
            _ => panic!("Expected GET command"),
        }
    }
    
    #[test]
    fn test_message_size_limit() {
        let config = ProtobufConfig {
            max_message_size: 100, // Very small limit
            ..Default::default()
        };
        let mut parser = ProtobufParser::new(config);
        
        // Create a large message
        let large_data = vec![0u8; 200]; // Larger than limit
        let data = Bytes::from(large_data);
        
        let result = parser.parse_command(data);
        assert!(result.is_err());
        
        match result.unwrap_err() {
            ProtobufError::MessageTooLarge { size: 200, max: 100 } => {},
            _ => panic!("Expected MessageTooLarge error"),
        }
    }
}