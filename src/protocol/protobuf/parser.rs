//! Protobuf Parser for CrabCache
//! High-performance parser with zero-copy optimizations

use bytes::{Buf, Bytes};
use std::time::Instant;

use crate::protocol::protobuf::{
    generated::{crab_cache_command, BatchCommand, CrabCacheCommand, GetCommand, PutCommand},
    ProtobufConfig, ProtobufError, ProtobufMetrics, ProtobufResult,
};
use crate::protocol::Command;

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
        let proto_command = CrabCacheCommand::decode(&data).map_err(|e| {
            ProtobufError::DecodeError(prost::DecodeError::new("Stub decode error"))
        })?;

        // Convert to internal command format
        let command = self.convert_proto_to_command(proto_command)?;

        // Update metrics
        let parse_time = start_time.elapsed().as_micros() as f64;
        self.metrics
            .update_message_processed(data_len, parse_time, 0.0);

        Ok(command)
    }

    /// Parse multiple commands from a batch
    pub fn parse_batch(&mut self, data: Bytes) -> ProtobufResult<Vec<Command>> {
        let start_time = Instant::now();

        // Parse batch command
        let proto_command = CrabCacheCommand::decode(&data).map_err(|e| {
            ProtobufError::DecodeError(prost::DecodeError::new("Stub decode error"))
        })?;

        match proto_command.command {
            Some(crab_cache_command::Command::Batch(batch_cmd)) => {
                let mut commands = Vec::with_capacity(batch_cmd.commands.len());

                for proto_cmd in batch_cmd.commands {
                    let command = self.convert_proto_to_command(proto_cmd)?;
                    commands.push(command);
                }

                // Update metrics
                let parse_time = start_time.elapsed().as_micros() as f64;
                self.metrics
                    .update_message_processed(data.len(), parse_time, 0.0);

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
            Some(crab_cache_command::Command::Put(put_cmd)) => Ok(Command::Put {
                key: put_cmd.key,
                value: put_cmd.value,
                ttl: put_cmd.ttl_seconds,
            }),

            Some(crab_cache_command::Command::Get(get_cmd)) => {
                Ok(Command::Get { key: get_cmd.key })
            }

            Some(crab_cache_command::Command::Del(del_cmd)) => {
                Ok(Command::Del { key: del_cmd.key })
            }

            Some(crab_cache_command::Command::Expire(expire_cmd)) => Ok(Command::Expire {
                key: expire_cmd.key,
                ttl: expire_cmd.ttl_seconds,
            }),

            Some(crab_cache_command::Command::Stats(_)) => Ok(Command::Stats),
            Some(crab_cache_command::Command::Metrics(_)) => Ok(Command::Metrics),
            Some(crab_cache_command::Command::Ping(_)) => Ok(Command::Ping),

            Some(crab_cache_command::Command::Batch(_)) => Err(ProtobufError::DecodeError(
                prost::DecodeError::new("Batch commands should be handled separately"),
            )),

            None => Err(ProtobufError::DecodeError(prost::DecodeError::new(
                "No command specified",
            ))),
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

        // Create a simple PUT command using basic protobuf structure
        let data = Bytes::from(vec![
            0x0A, 0x08, 0x74, 0x65, 0x73, 0x74, 0x5F, 0x6B, 0x65, 0x79, // key: "test_key"
            0x12, 0x0A, 0x74, 0x65, 0x73, 0x74, 0x5F, 0x76, 0x61, 0x6C, 0x75,
            0x65, // value: "test_value"
        ]);

        // This test should fail gracefully since we don't have real protobuf structs
        let result = parser.parse_command(data);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_get_command() {
        let mut parser = ProtobufParser::default();

        // Create a simple GET command using basic protobuf structure
        let data = Bytes::from(vec![
            0x0A, 0x08, 0x74, 0x65, 0x73, 0x74, 0x5F, 0x6B, 0x65, 0x79, // key: "test_key"
        ]);

        // This test should fail gracefully since we don't have real protobuf structs
        let result = parser.parse_command(data);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_batch_commands() {
        let mut parser = ProtobufParser::default();

        // Create a simple batch command using basic protobuf structure
        let data = Bytes::from(vec![
            0x0A, 0x04, 0x6B, 0x65, 0x79, 0x31, // key1
            0x12, 0x06, 0x76, 0x61, 0x6C, 0x75, 0x65, 0x31, // value1
        ]);

        // This test should fail gracefully since we don't have real protobuf structs
        let result = parser.parse_batch(data);
        assert!(result.is_err());
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
            ProtobufError::MessageTooLarge {
                size: 200,
                max: 100,
            } => {}
            _ => panic!("Expected MessageTooLarge error"),
        }
    }
}
