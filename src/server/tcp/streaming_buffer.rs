//! Streaming buffer for handling large commands that exceed single buffer size

use bytes::{BytesMut, BufMut};
use std::collections::VecDeque;

/// Streaming buffer that can accumulate data across multiple reads
pub struct StreamingBuffer {
    /// Accumulated data
    data: BytesMut,
    /// Maximum allowed size
    max_size: usize,
    /// Expected size (if known from protocol)
    expected_size: Option<usize>,
    /// Command boundaries (for text protocol)
    command_boundaries: VecDeque<usize>,
}

impl StreamingBuffer {
    /// Create a new streaming buffer
    pub fn new(max_size: usize) -> Self {
        Self {
            data: BytesMut::with_capacity(8192), // Start with 8KB
            max_size,
            expected_size: None,
            command_boundaries: VecDeque::new(),
        }
    }

    /// Append new data to the buffer
    pub fn append(&mut self, new_data: &[u8]) -> Result<(), String> {
        // Check size limits
        if self.data.len() + new_data.len() > self.max_size {
            return Err(format!(
                "Command too large: {} bytes (max: {})",
                self.data.len() + new_data.len(),
                self.max_size
            ));
        }

        // Append data
        self.data.put_slice(new_data);

        // Update command boundaries for text protocol
        self.update_command_boundaries();

        Ok(())
    }

    /// Check if we have complete commands available
    pub fn has_complete_commands(&self) -> bool {
        !self.command_boundaries.is_empty()
    }

    /// Extract the next complete command
    pub fn extract_command(&mut self) -> Option<Vec<u8>> {
        if let Some(boundary) = self.command_boundaries.pop_front() {
            // Check if this is a binary protocol packet (TOON, CRAB, or binary command)
            let is_binary_protocol = self.data.len() >= 4 && 
                (&self.data[0..4] == b"TOON" || &self.data[0..4] == b"CRAB");
            
            let is_binary_command = self.data.len() > 0 && 
                self.data[0] >= 0x01 && self.data[0] <= 0x06;
            
            if is_binary_protocol || is_binary_command {
                // For binary protocols/commands, extract exact number of bytes (boundary + 1)
                let command_data = self.data.split_to(boundary + 1);
                let result = command_data.to_vec();
                
                // Update remaining boundaries
                for boundary_ref in &mut self.command_boundaries {
                    *boundary_ref -= boundary + 1;
                }
                
                Some(result)
            } else {
                // For text protocols, extract and remove newlines
                let command_data = self.data.split_to(boundary + 1); // +1 to include newline
                
                // Remove the newline for text protocols
                let mut result = command_data.to_vec();
                if result.ends_with(b"\n") {
                    result.pop();
                }
                if result.ends_with(b"\r") {
                    result.pop();
                }

                // Update remaining boundaries
                for boundary_ref in &mut self.command_boundaries {
                    *boundary_ref -= boundary + 1;
                }

                Some(result)
            }
        } else {
            None
        }
    }

    /// Get current buffer size
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.data.clear();
        self.command_boundaries.clear();
        self.expected_size = None;
    }

    /// Set expected size (for binary protocols)
    pub fn set_expected_size(&mut self, size: usize) {
        self.expected_size = Some(size);
    }

    /// Check if we have enough data for expected size
    pub fn has_expected_data(&self) -> bool {
        if let Some(expected) = self.expected_size {
            self.data.len() >= expected
        } else {
            false
        }
    }

    /// Extract expected amount of data
    pub fn extract_expected(&mut self) -> Option<Vec<u8>> {
        if let Some(expected) = self.expected_size {
            if self.data.len() >= expected {
                let result = self.data.split_to(expected).to_vec();
                self.expected_size = None;
                return Some(result);
            }
        }
        None
    }

    /// Update command boundaries by looking for newlines and binary protocols
    fn update_command_boundaries(&mut self) {
        let data_slice = &self.data[..];
        let mut search_start = if let Some(&last_boundary) = self.command_boundaries.back() {
            last_boundary + 1
        } else {
            0
        };

        // Check for TOON protocol negotiation (6 bytes: TOON + version + flags)
        if data_slice.len() >= 6 && &data_slice[0..4] == b"TOON" {
            // TOON protocol negotiation packet is exactly 6 bytes
            if data_slice.len() == 6 {
                self.command_boundaries.push_back(5); // Mark end of 6-byte packet
                return;
            }
            // For longer TOON packets, we'd need to parse the length field
            // For now, handle negotiation case
        }

        // Check for Protobuf protocol negotiation (6 bytes: CRAB + version + flags)
        if data_slice.len() >= 6 && &data_slice[0..4] == b"CRAB" {
            // Protobuf protocol negotiation packet is exactly 6 bytes
            if data_slice.len() == 6 {
                self.command_boundaries.push_back(5); // Mark end of 6-byte packet
                return;
            }
        }

        // Check for binary protocol commands (0x01-0x06)
        if data_slice.len() > 0 {
            let first_byte = data_slice[0];
            if first_byte >= 0x01 && first_byte <= 0x06 {
                // This is a binary protocol command - parse it properly
                if let Some(command_end) = self.parse_binary_command_length(data_slice) {
                    self.command_boundaries.push_back(command_end);
                    return;
                }
            }
        }

        // Look for newlines starting from where we left off (text protocol)
        while let Some(pos) = data_slice[search_start..].iter().position(|&b| b == b'\n') {
            let absolute_pos = search_start + pos;
            self.command_boundaries.push_back(absolute_pos);
            search_start = absolute_pos + 1;
        }
    }

    /// Parse binary command to determine its length
    fn parse_binary_command_length(&self, data: &[u8]) -> Option<usize> {
        if data.is_empty() {
            return None;
        }

        let cmd_type = data[0];

        match cmd_type {
            0x01 => Some(0), // PING - just 1 byte (0x01)
            0x06 => Some(0), // STATS - just 1 byte (0x06)
            
            0x02 => { // PUT - cmd + key_len(4) + key + value_len(4) + value + ttl_flag(1) + [ttl(8)]
                if data.len() < 5 { return None; } // Need at least cmd + key_len
                
                // Read key length safely
                let key_len = if data.len() >= 5 {
                    u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize
                } else {
                    return None;
                };
                
                let mut cursor = 5 + key_len; // cmd(1) + key_len(4) + key
                
                if data.len() < cursor + 4 { return None; } // Need value_len
                
                // Read value length safely
                let value_len = if data.len() >= cursor + 4 {
                    u32::from_le_bytes([
                        data[cursor], data[cursor+1], data[cursor+2], data[cursor+3]
                    ]) as usize
                } else {
                    return None;
                };
                
                cursor += 4 + value_len; // value_len(4) + value
                
                if data.len() < cursor + 1 { return None; } // Need TTL flag
                
                let has_ttl = data[cursor] == 1;
                cursor += 1;
                
                if has_ttl {
                    cursor += 8; // TTL is 8 bytes
                }
                
                if data.len() >= cursor {
                    Some(cursor - 1) // -1 because boundary is inclusive
                } else {
                    None
                }
            },
            
            0x03 | 0x04 => { // GET/DEL - cmd + key_len(4) + key
                if data.len() < 5 { return None; } // Need at least cmd + key_len
                
                let key_len = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;
                let cursor = 5 + key_len; // cmd(1) + key_len(4) + key
                
                if data.len() >= cursor {
                    Some(cursor - 1) // -1 because boundary is inclusive
                } else {
                    None
                }
            },
            
            0x05 => { // EXPIRE - cmd + key_len(4) + key + ttl(8)
                if data.len() < 5 { return None; } // Need at least cmd + key_len
                
                let key_len = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;
                let cursor = 5 + key_len + 8; // cmd(1) + key_len(4) + key + ttl(8)
                
                if data.len() >= cursor {
                    Some(cursor - 1) // -1 because boundary is inclusive
                } else {
                    None
                }
            },
            
            _ => None, // Unknown command
        }
    }

    /// Get remaining capacity
    pub fn remaining_capacity(&self) -> usize {
        self.max_size.saturating_sub(self.data.len())
    }

    /// Check if buffer is near capacity
    pub fn is_near_capacity(&self, threshold: f64) -> bool {
        let usage_ratio = self.data.len() as f64 / self.max_size as f64;
        usage_ratio >= threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_buffer_single_command() {
        let mut buffer = StreamingBuffer::new(1024);
        
        // Add a complete command
        buffer.append(b"GET key1\n").unwrap();
        
        assert!(buffer.has_complete_commands());
        
        let command = buffer.extract_command().unwrap();
        assert_eq!(command, b"GET key1");
        
        assert!(!buffer.has_complete_commands());
    }

    #[test]
    fn test_streaming_buffer_multiple_commands() {
        let mut buffer = StreamingBuffer::new(1024);
        
        // Add multiple commands
        buffer.append(b"GET key1\nPUT key2 value2\nDEL key3\n").unwrap();
        
        assert!(buffer.has_complete_commands());
        
        // Extract first command
        let cmd1 = buffer.extract_command().unwrap();
        assert_eq!(cmd1, b"GET key1");
        
        // Extract second command
        let cmd2 = buffer.extract_command().unwrap();
        assert_eq!(cmd2, b"PUT key2 value2");
        
        // Extract third command
        let cmd3 = buffer.extract_command().unwrap();
        assert_eq!(cmd3, b"DEL key3");
        
        assert!(!buffer.has_complete_commands());
    }

    #[test]
    fn test_streaming_buffer_partial_command() {
        let mut buffer = StreamingBuffer::new(1024);
        
        // Add partial command
        buffer.append(b"GET key").unwrap();
        assert!(!buffer.has_complete_commands());
        
        // Complete the command
        buffer.append(b"1\n").unwrap();
        assert!(buffer.has_complete_commands());
        
        let command = buffer.extract_command().unwrap();
        assert_eq!(command, b"GET key1");
    }

    #[test]
    fn test_streaming_buffer_size_limit() {
        let mut buffer = StreamingBuffer::new(10); // Very small limit
        
        // Try to add data that exceeds limit
        let result = buffer.append(b"This is a very long command that exceeds the limit");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Command too large"));
    }

    #[test]
    fn test_streaming_buffer_expected_size() {
        let mut buffer = StreamingBuffer::new(1024);
        
        // Set expected size
        buffer.set_expected_size(10);
        
        // Add partial data
        buffer.append(b"12345").unwrap();
        assert!(!buffer.has_expected_data());
        
        // Complete expected data
        buffer.append(b"67890").unwrap();
        assert!(buffer.has_expected_data());
        
        let data = buffer.extract_expected().unwrap();
        assert_eq!(data, b"1234567890");
    }

    #[test]
    fn test_streaming_buffer_with_quotes() {
        let mut buffer = StreamingBuffer::new(1024);
        
        // Add command with quoted value containing spaces
        buffer.append(b"PUT key \"Hello World\"\n").unwrap();
        
        assert!(buffer.has_complete_commands());
        
        let command = buffer.extract_command().unwrap();
        assert_eq!(command, b"PUT key \"Hello World\"");
    }

    #[test]
    fn test_streaming_buffer_large_value() {
        let mut buffer = StreamingBuffer::new(10240); // 10KB limit
        
        // Create a large value (5KB)
        let large_value = "x".repeat(5000);
        let command = format!("PUT large_key \"{}\"\n", large_value);
        
        buffer.append(command.as_bytes()).unwrap();
        
        assert!(buffer.has_complete_commands());
        
        let extracted = buffer.extract_command().unwrap();
        let extracted_str = String::from_utf8(extracted).unwrap();
        assert!(extracted_str.starts_with("PUT large_key"));
        assert!(extracted_str.contains(&large_value));
    }
}