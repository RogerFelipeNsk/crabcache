//! Pipeline protocol implementation for batch command processing

use crate::protocol::commands::{Command, Response};
use crate::protocol::{ProtocolParser, ProtocolSerializer, BinaryProtocol};
use std::collections::VecDeque;
use tracing::{debug, warn};

/// Pipeline processor for batch command handling
pub struct PipelineProcessor {
    /// Maximum batch size for pipeline processing
    max_batch_size: usize,
    /// Buffer for accumulating commands
    command_buffer: VecDeque<Command>,
    /// Buffer for accumulating responses
    response_buffer: VecDeque<Response>,
    /// Statistics
    stats: PipelineStats,
}

/// Pipeline processing statistics
#[derive(Debug, Clone, Default)]
pub struct PipelineStats {
    pub total_batches: u64,
    pub total_commands: u64,
    pub avg_batch_size: f64,
    pub max_batch_size: usize,
    pub parse_errors: u64,
    pub processing_errors: u64,
}

/// Pipeline batch containing commands and metadata
#[derive(Debug, Clone)]
pub struct PipelineBatch {
    pub commands: Vec<Command>,
    pub batch_id: u64,
    pub timestamp: std::time::Instant,
    pub use_binary_protocol: bool,
}

/// Pipeline response batch
#[derive(Debug, Clone)]
pub struct PipelineResponseBatch {
    pub responses: Vec<Response>,
    pub batch_id: u64,
    pub use_binary_protocol: bool,
}

impl PipelineProcessor {
    /// Create new pipeline processor
    pub fn new(max_batch_size: usize) -> Self {
        Self {
            max_batch_size: max_batch_size.max(1).min(1000), // Limit between 1-1000
            command_buffer: VecDeque::new(),
            response_buffer: VecDeque::new(),
            stats: PipelineStats::default(),
        }
    }

    /// Parse multiple commands from buffer
    pub fn parse_batch(&mut self, data: &[u8]) -> Result<PipelineBatch, String> {
        if data.is_empty() {
            return Err("Empty batch data".to_string());
        }

        let mut commands = Vec::new();
        let mut offset = 0;
        let mut use_binary_protocol = false;
        let batch_id = self.stats.total_batches;

        // Try to detect protocol type from first command
        if data.len() > 0 {
            let first_byte = data[0];
            use_binary_protocol = first_byte >= 0x01 && first_byte <= 0x06;
        }

        debug!("Parsing batch with {} bytes, binary={}", data.len(), use_binary_protocol);

        // Parse commands until buffer is exhausted or max batch size reached
        while offset < data.len() && commands.len() < self.max_batch_size {
            let remaining = &data[offset..];
            
            if remaining.is_empty() {
                break;
            }

            let (command, bytes_consumed) = if use_binary_protocol {
                self.parse_binary_command(remaining)?
            } else {
                self.parse_text_command(remaining)?
            };

            commands.push(command);
            offset += bytes_consumed;

            // Safety check to prevent infinite loops
            if bytes_consumed == 0 {
                warn!("Command parser consumed 0 bytes, breaking to prevent infinite loop");
                break;
            }
        }

        if commands.is_empty() {
            self.stats.parse_errors += 1;
            return Err("No valid commands found in batch".to_string());
        }

        // Update statistics
        self.stats.total_batches += 1;
        self.stats.total_commands += commands.len() as u64;
        self.stats.avg_batch_size = self.stats.total_commands as f64 / self.stats.total_batches as f64;
        self.stats.max_batch_size = self.stats.max_batch_size.max(commands.len());

        debug!("Parsed batch: {} commands, binary={}", commands.len(), use_binary_protocol);

        Ok(PipelineBatch {
            commands,
            batch_id,
            timestamp: std::time::Instant::now(),
            use_binary_protocol,
        })
    }

    /// Parse single binary command from buffer
    fn parse_binary_command(&self, data: &[u8]) -> Result<(Command, usize), String> {
        match BinaryProtocol::parse_command(data) {
            Ok(command) => {
                // Calculate bytes consumed (simplified - in real implementation, 
                // BinaryProtocol should return bytes consumed)
                let bytes_consumed = self.estimate_binary_command_size(&command);
                Ok((command, bytes_consumed))
            }
            Err(e) => Err(format!("Binary command parse error: {}", e)),
        }
    }

    /// Parse single text command from buffer
    fn parse_text_command(&self, data: &[u8]) -> Result<(Command, usize), String> {
        // Find the end of the command (newline)
        if let Some(newline_pos) = data.iter().position(|&b| b == b'\n') {
            let command_bytes = &data[..newline_pos];
            let _command_str = std::str::from_utf8(command_bytes)
                .map_err(|e| format!("Invalid UTF-8 in command: {}", e))?;

            match ProtocolParser::parse_command(command_bytes) {
                Ok(command) => Ok((command, newline_pos + 1)), // +1 for newline
                Err(e) => Err(format!("Text command parse error: {}", e)),
            }
        } else {
            Err("Incomplete command: no newline found".to_string())
        }
    }

    /// Estimate binary command size (simplified implementation)
    fn estimate_binary_command_size(&self, command: &Command) -> usize {
        match command {
            Command::Ping => 1,
            Command::Put { key, value, .. } => 1 + 4 + key.len() + 4 + value.len(),
            Command::Get { key } => 1 + 4 + key.len(),
            Command::Del { key } => 1 + 4 + key.len(),
            Command::Expire { key, .. } => 1 + 4 + key.len() + 8,
            Command::Stats => 1,
            Command::Metrics => 1,
        }
    }

    /// Serialize batch of responses
    pub fn serialize_response_batch(&self, batch: &PipelineResponseBatch) -> Result<Vec<u8>, String> {
        if batch.responses.is_empty() {
            return Ok(Vec::new());
        }

        let mut buffer = Vec::new();

        if batch.use_binary_protocol {
            // Serialize as binary batch
            for response in &batch.responses {
                let response_bytes = BinaryProtocol::serialize_response(response);
                buffer.extend_from_slice(&response_bytes);
            }
        } else {
            // Serialize as text batch
            for response in &batch.responses {
                let response_bytes = ProtocolSerializer::serialize_response(response)
                    .map_err(|e| format!("Failed to serialize response: {}", e))?;
                buffer.extend_from_slice(&response_bytes);
            }
        }

        debug!("Serialized response batch: {} responses, {} bytes", 
               batch.responses.len(), buffer.len());

        Ok(buffer)
    }

    /// Get pipeline statistics
    pub fn get_stats(&self) -> &PipelineStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = PipelineStats::default();
    }

    /// Get optimal batch size based on current performance
    pub fn get_optimal_batch_size(&self) -> usize {
        // Start with a reasonable default
        let mut optimal_size = 16;

        // Adjust based on average batch size and error rate
        if self.stats.total_batches > 100 {
            let error_rate = (self.stats.parse_errors + self.stats.processing_errors) as f64 
                           / self.stats.total_batches as f64;

            if error_rate < 0.01 {
                // Low error rate, can increase batch size
                optimal_size = (self.stats.avg_batch_size * 1.2) as usize;
            } else if error_rate > 0.05 {
                // High error rate, decrease batch size
                optimal_size = (self.stats.avg_batch_size * 0.8) as usize;
            } else {
                // Moderate error rate, keep current average
                optimal_size = self.stats.avg_batch_size as usize;
            }
        }

        optimal_size.max(1).min(self.max_batch_size)
    }
}

/// Pipeline builder for constructing batches
pub struct PipelineBuilder {
    commands: Vec<Command>,
    max_size: usize,
    use_binary: bool,
}

impl PipelineBuilder {
    /// Create new pipeline builder
    pub fn new(max_size: usize, use_binary: bool) -> Self {
        Self {
            commands: Vec::with_capacity(max_size),
            max_size,
            use_binary,
        }
    }

    /// Add command to batch
    pub fn add_command(&mut self, command: Command) -> Result<(), String> {
        if self.commands.len() >= self.max_size {
            return Err("Batch is full".to_string());
        }

        self.commands.push(command);
        Ok(())
    }

    /// Build the batch
    pub fn build(self) -> PipelineBatch {
        PipelineBatch {
            commands: self.commands,
            batch_id: 0, // Will be set by processor
            timestamp: std::time::Instant::now(),
            use_binary_protocol: self.use_binary,
        }
    }

    /// Check if batch is full
    pub fn is_full(&self) -> bool {
        self.commands.len() >= self.max_size
    }

    /// Get current batch size
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// Pipeline protocol trait for different protocol implementations
pub trait PipelineProtocol {
    /// Parse batch of commands from buffer
    fn parse_batch(&self, data: &[u8]) -> Result<Vec<Command>, String>;
    
    /// Serialize batch of responses to buffer
    fn serialize_batch(&self, responses: &[Response]) -> Result<Vec<u8>, String>;
    
    /// Get protocol name
    fn protocol_name(&self) -> &'static str;
}

/// Binary pipeline protocol implementation
pub struct BinaryPipelineProtocol;

impl PipelineProtocol for BinaryPipelineProtocol {
    fn parse_batch(&self, data: &[u8]) -> Result<Vec<Command>, String> {
        let mut commands = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            match BinaryProtocol::parse_command(&data[offset..]) {
                Ok(command) => {
                    commands.push(command);
                    // This is simplified - real implementation needs to track bytes consumed
                    offset += 1; // Placeholder
                }
                Err(_) => break,
            }
        }

        Ok(commands)
    }

    fn serialize_batch(&self, responses: &[Response]) -> Result<Vec<u8>, String> {
        let mut buffer = Vec::new();
        
        for response in responses {
            let response_bytes = BinaryProtocol::serialize_response(response);
            buffer.extend_from_slice(&response_bytes);
        }

        Ok(buffer)
    }

    fn protocol_name(&self) -> &'static str {
        "binary"
    }
}

/// Text pipeline protocol implementation
pub struct TextPipelineProtocol;

impl PipelineProtocol for TextPipelineProtocol {
    fn parse_batch(&self, data: &[u8]) -> Result<Vec<Command>, String> {
        let mut commands = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            if let Some(newline_pos) = data[offset..].iter().position(|&b| b == b'\n') {
                let command_end = offset + newline_pos;
                let command_bytes = &data[offset..command_end];
                
                match ProtocolParser::parse_command(command_bytes) {
                    Ok(command) => {
                        commands.push(command);
                        offset = command_end + 1; // Skip newline
                    }
                    Err(_) => break,
                }
            } else {
                break; // No more complete commands
            }
        }

        Ok(commands)
    }

    fn serialize_batch(&self, responses: &[Response]) -> Result<Vec<u8>, String> {
        let mut buffer = Vec::new();
        
        for response in responses {
            let response_bytes = ProtocolSerializer::serialize_response(response)
                .map_err(|e| format!("Serialization error: {}", e))?;
            buffer.extend_from_slice(&response_bytes);
        }

        Ok(buffer)
    }

    fn protocol_name(&self) -> &'static str {
        "text"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn test_pipeline_processor_creation() {
        let processor = PipelineProcessor::new(16);
        assert_eq!(processor.max_batch_size, 16);
        assert_eq!(processor.stats.total_batches, 0);
    }

    #[test]
    fn test_pipeline_builder() {
        let mut builder = PipelineBuilder::new(3, false);
        
        assert!(builder.is_empty());
        assert!(!builder.is_full());
        
        builder.add_command(Command::Ping).unwrap();
        builder.add_command(Command::Get { key: Bytes::from("test") }).unwrap();
        builder.add_command(Command::Put { 
            key: Bytes::from("key"), 
            value: Bytes::from("value"), 
            ttl: None 
        }).unwrap();
        
        assert!(builder.is_full());
        assert_eq!(builder.len(), 3);
        
        // Should fail to add more commands
        assert!(builder.add_command(Command::Ping).is_err());
        
        let batch = builder.build();
        assert_eq!(batch.commands.len(), 3);
    }

    #[test]
    fn test_text_pipeline_protocol() {
        let protocol = TextPipelineProtocol;
        
        let batch_data = b"PING\nGET test\nPUT key value\n";
        let commands = protocol.parse_batch(batch_data).unwrap();
        
        assert_eq!(commands.len(), 3);
        assert!(matches!(commands[0], Command::Ping));
        assert!(matches!(commands[1], Command::Get { .. }));
        assert!(matches!(commands[2], Command::Put { .. }));
    }

    #[test]
    fn test_pipeline_stats() {
        let mut processor = PipelineProcessor::new(10);
        
        // Simulate processing some batches
        processor.stats.total_batches = 5;
        processor.stats.total_commands = 50;
        processor.stats.avg_batch_size = 10.0;
        processor.stats.max_batch_size = 15;
        
        let optimal_size = processor.get_optimal_batch_size();
        assert!(optimal_size > 0);
        assert!(optimal_size <= processor.max_batch_size);
    }
}