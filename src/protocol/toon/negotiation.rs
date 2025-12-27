//! TOON Protocol Negotiation
//! Automatic protocol selection and capability detection

use super::{ToonFlags, ToonPacket, ToonType, TOON_MAGIC, TOON_VERSION};
use bytes::{BufMut, BytesMut};
use std::collections::HashMap;

/// Protocol negotiation result
#[derive(Debug, Clone, PartialEq)]
pub enum ToonNegotiationResult {
    /// TOON protocol accepted with specific capabilities
    ToonAccepted {
        version: u8,
        flags: ToonFlags,
        capabilities: ToonCapabilities,
    },
    /// Fallback to Protobuf protocol
    ProtobufFallback,
    /// Fallback to text protocol
    TextFallback,
    /// Negotiation failed
    Failed(String),
}

/// TOON Protocol Capabilities
#[derive(Debug, Clone, PartialEq)]
pub struct ToonCapabilities {
    pub zero_copy: bool,
    pub string_interning: bool,
    pub compression: bool,
    pub simd_optimized: bool,
    pub max_packet_size: usize,
    pub supported_types: Vec<u8>,
}

impl Default for ToonCapabilities {
    fn default() -> Self {
        Self {
            zero_copy: true,
            string_interning: true,
            compression: false,
            simd_optimized: cfg!(target_feature = "avx2"),
            max_packet_size: 16 * 1024 * 1024, // 16MB
            supported_types: vec![
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
                0x0E, 0x0F, 0x10,
            ],
        }
    }
}

/// TOON Protocol Negotiator
pub struct ToonNegotiator {
    capabilities: ToonCapabilities,
}

impl Default for ToonNegotiator {
    fn default() -> Self {
        Self::new()
    }
}

impl ToonNegotiator {
    pub fn new() -> Self {
        Self {
            capabilities: ToonCapabilities::default(),
        }
    }

    pub fn with_capabilities(capabilities: ToonCapabilities) -> Self {
        Self { capabilities }
    }

    /// Create negotiation request packet
    pub fn create_negotiation_request(&self) -> BytesMut {
        let mut buf = BytesMut::with_capacity(64);

        // Write TOON magic bytes
        buf.put_slice(TOON_MAGIC);

        // Write version
        buf.put_u8(TOON_VERSION);

        // Write desired flags
        let flags = ToonFlags {
            zero_copy: self.capabilities.zero_copy,
            string_interning: self.capabilities.string_interning,
            compression: self.capabilities.compression,
            simd_optimized: self.capabilities.simd_optimized,
        };
        buf.put_u8(flags.to_byte());

        // Write capabilities as TOON object
        let mut caps_obj = HashMap::new();
        caps_obj.insert(
            "max_packet_size".to_string(),
            ToonType::UInt32(self.capabilities.max_packet_size as u32),
        );
        caps_obj.insert(
            "supported_types".to_string(),
            ToonType::Array(
                self.capabilities
                    .supported_types
                    .iter()
                    .map(|&t| ToonType::UInt8(t))
                    .collect(),
            ),
        );

        let caps_packet = ToonPacket::new(ToonType::Object(caps_obj));

        // Encode capabilities (simplified for negotiation)
        let caps_data = self.encode_capabilities_simple();

        // Write length and data
        buf.put_u8(caps_data.len() as u8);
        buf.put_slice(&caps_data);

        buf
    }

    /// Process negotiation request and create response
    pub fn process_negotiation_request(
        &self,
        request: &[u8],
    ) -> (ToonNegotiationResult, Option<BytesMut>) {
        if request.len() < 6 {
            return (
                ToonNegotiationResult::Failed("Request too short".to_string()),
                None,
            );
        }

        // Check magic bytes
        if &request[0..4] != TOON_MAGIC {
            // Not a TOON request - check for other protocols
            if &request[0..4] == b"CRAB" {
                return (ToonNegotiationResult::ProtobufFallback, None);
            } else {
                return (ToonNegotiationResult::TextFallback, None);
            }
        }

        // Check version
        let version = request[4];
        if version != TOON_VERSION {
            return (
                ToonNegotiationResult::Failed(format!("Unsupported version: {}", version)),
                None,
            );
        }

        // Parse flags
        let client_flags = ToonFlags::from_byte(request[5]);

        // Negotiate capabilities
        let negotiated_flags = ToonFlags {
            zero_copy: self.capabilities.zero_copy && client_flags.zero_copy,
            string_interning: self.capabilities.string_interning && client_flags.string_interning,
            compression: self.capabilities.compression && client_flags.compression,
            simd_optimized: self.capabilities.simd_optimized && client_flags.simd_optimized,
        };

        let negotiated_caps = ToonCapabilities {
            zero_copy: negotiated_flags.zero_copy,
            string_interning: negotiated_flags.string_interning,
            compression: negotiated_flags.compression,
            simd_optimized: negotiated_flags.simd_optimized,
            max_packet_size: self.capabilities.max_packet_size,
            supported_types: self.capabilities.supported_types.clone(),
        };

        // Create response
        let response = self.create_negotiation_response(&negotiated_flags, &negotiated_caps);

        (
            ToonNegotiationResult::ToonAccepted {
                version: TOON_VERSION,
                flags: negotiated_flags,
                capabilities: negotiated_caps,
            },
            Some(response),
        )
    }

    /// Create negotiation response packet
    fn create_negotiation_response(&self, flags: &ToonFlags, caps: &ToonCapabilities) -> BytesMut {
        let mut buf = BytesMut::with_capacity(64);

        // Write TOON magic bytes (confirmation)
        buf.put_slice(TOON_MAGIC);

        // Write version
        buf.put_u8(TOON_VERSION);

        // Write negotiated flags
        buf.put_u8(flags.to_byte());

        // Write "ACCEPTED" marker
        buf.put_slice(b"ACCEPTED");

        // Write negotiated capabilities
        let caps_data = self.encode_capabilities_for_response(caps);
        buf.put_u8(caps_data.len() as u8);
        buf.put_slice(&caps_data);

        buf
    }

    /// Process negotiation response
    pub fn process_negotiation_response(&self, response: &[u8]) -> ToonNegotiationResult {
        if response.len() < 14 {
            // TOON + version + flags + ACCEPTED
            return ToonNegotiationResult::Failed("Response too short".to_string());
        }

        // Check magic bytes
        if &response[0..4] != TOON_MAGIC {
            return ToonNegotiationResult::Failed("Invalid magic bytes in response".to_string());
        }

        // Check version
        let version = response[4];
        if version != TOON_VERSION {
            return ToonNegotiationResult::Failed(format!(
                "Unsupported version in response: {}",
                version
            ));
        }

        // Parse negotiated flags
        let flags = ToonFlags::from_byte(response[5]);

        // Check for ACCEPTED marker
        if &response[6..14] != b"ACCEPTED" {
            return ToonNegotiationResult::Failed("Server rejected TOON protocol".to_string());
        }

        // Parse capabilities if present
        let capabilities = if response.len() > 15 {
            let caps_len = response[14] as usize;
            if response.len() >= 15 + caps_len {
                self.decode_capabilities(&response[15..15 + caps_len])
                    .unwrap_or_else(|_| ToonCapabilities::default())
            } else {
                ToonCapabilities::default()
            }
        } else {
            ToonCapabilities::default()
        };

        ToonNegotiationResult::ToonAccepted {
            version,
            flags,
            capabilities,
        }
    }

    /// Encode capabilities for negotiation (simplified format)
    fn encode_capabilities_simple(&self) -> Vec<u8> {
        let mut data = Vec::new();

        // Max packet size (4 bytes)
        data.extend_from_slice(&(self.capabilities.max_packet_size as u32).to_le_bytes());

        // Supported types count + types
        data.push(self.capabilities.supported_types.len() as u8);
        data.extend_from_slice(&self.capabilities.supported_types);

        data
    }

    /// Encode capabilities for response
    fn encode_capabilities_for_response(&self, caps: &ToonCapabilities) -> Vec<u8> {
        let mut data = Vec::new();

        // Max packet size (4 bytes)
        data.extend_from_slice(&(caps.max_packet_size as u32).to_le_bytes());

        // Supported types count + types
        data.push(caps.supported_types.len() as u8);
        data.extend_from_slice(&caps.supported_types);

        data
    }

    /// Decode capabilities from bytes
    fn decode_capabilities(&self, data: &[u8]) -> Result<ToonCapabilities, String> {
        if data.len() < 5 {
            return Err("Capabilities data too short".to_string());
        }

        let mut cursor = 0;

        // Read max packet size
        let max_packet_size = u32::from_le_bytes([
            data[cursor],
            data[cursor + 1],
            data[cursor + 2],
            data[cursor + 3],
        ]) as usize;
        cursor += 4;

        // Read supported types
        let types_count = data[cursor] as usize;
        cursor += 1;

        if cursor + types_count > data.len() {
            return Err("Invalid supported types data".to_string());
        }

        let supported_types = data[cursor..cursor + types_count].to_vec();

        Ok(ToonCapabilities {
            zero_copy: self.capabilities.zero_copy,
            string_interning: self.capabilities.string_interning,
            compression: self.capabilities.compression,
            simd_optimized: self.capabilities.simd_optimized,
            max_packet_size,
            supported_types,
        })
    }

    /// Check if client supports TOON protocol
    pub fn is_toon_request(data: &[u8]) -> bool {
        data.len() >= 4 && &data[0..4] == TOON_MAGIC
    }

    /// Check if this is a Protobuf fallback request
    pub fn is_protobuf_request(data: &[u8]) -> bool {
        data.len() >= 4 && &data[0..4] == b"CRAB"
    }

    /// Get protocol priority order
    pub fn get_protocol_priority() -> Vec<&'static str> {
        vec!["TOON", "Protobuf", "Text"]
    }
}

/// TOON Protocol Detection Utility
pub struct ToonProtocolDetector;

impl ToonProtocolDetector {
    /// Detect protocol from incoming data
    pub fn detect_protocol(data: &[u8]) -> &'static str {
        if data.len() >= 4 {
            match &data[0..4] {
                b"TOON" => "TOON",
                b"CRAB" => "Protobuf",
                _ => "Text",
            }
        } else {
            "Text"
        }
    }

    /// Get protocol efficiency score (higher is better)
    pub fn get_protocol_efficiency(protocol: &str) -> u32 {
        match protocol {
            "TOON" => 100,    // Most efficient
            "Protobuf" => 70, // Good efficiency
            "Text" => 30,     // Least efficient
            _ => 0,
        }
    }

    /// Recommend best protocol for data size
    pub fn recommend_protocol(data_size: usize) -> &'static str {
        if data_size > 1024 {
            "TOON" // Best for large data
        } else if data_size > 100 {
            "Protobuf" // Good for medium data
        } else {
            "Text" // Acceptable for small data
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_negotiation_request_creation() {
        let negotiator = ToonNegotiator::new();
        let request = negotiator.create_negotiation_request();

        assert_eq!(&request[0..4], TOON_MAGIC);
        assert_eq!(request[4], TOON_VERSION);
        assert!(request.len() > 6);
    }

    #[test]
    fn test_negotiation_process() {
        let negotiator = ToonNegotiator::new();
        let request = negotiator.create_negotiation_request();

        let (result, response) = negotiator.process_negotiation_request(&request);

        match result {
            ToonNegotiationResult::ToonAccepted {
                version,
                flags,
                capabilities,
            } => {
                assert_eq!(version, TOON_VERSION);
                assert!(response.is_some());
            }
            _ => panic!("Expected TOON accepted"),
        }
    }

    #[test]
    fn test_protocol_detection() {
        assert_eq!(ToonProtocolDetector::detect_protocol(b"TOON123"), "TOON");
        assert_eq!(
            ToonProtocolDetector::detect_protocol(b"CRAB123"),
            "Protobuf"
        );
        assert_eq!(ToonProtocolDetector::detect_protocol(b"PING"), "Text");
        assert_eq!(ToonProtocolDetector::detect_protocol(b"GET key"), "Text");
    }

    #[test]
    fn test_protocol_efficiency() {
        assert_eq!(ToonProtocolDetector::get_protocol_efficiency("TOON"), 100);
        assert_eq!(
            ToonProtocolDetector::get_protocol_efficiency("Protobuf"),
            70
        );
        assert_eq!(ToonProtocolDetector::get_protocol_efficiency("Text"), 30);
    }

    #[test]
    fn test_protocol_recommendation() {
        assert_eq!(ToonProtocolDetector::recommend_protocol(2048), "TOON");
        assert_eq!(ToonProtocolDetector::recommend_protocol(512), "Protobuf");
        assert_eq!(ToonProtocolDetector::recommend_protocol(50), "Text");
    }

    #[test]
    fn test_fallback_detection() {
        let negotiator = ToonNegotiator::new();

        // Test Protobuf fallback
        let (result, _) = negotiator.process_negotiation_request(b"CRAB\x01\x00");
        assert_eq!(result, ToonNegotiationResult::ProtobufFallback);

        // Test text fallback
        let (result, _) = negotiator.process_negotiation_request(b"PING");
        assert_eq!(result, ToonNegotiationResult::TextFallback);
    }

    #[test]
    fn test_capabilities_encoding() {
        let negotiator = ToonNegotiator::new();
        let caps_data = negotiator.encode_capabilities_simple();

        assert!(caps_data.len() >= 5); // At least size + count

        let decoded = negotiator.decode_capabilities(&caps_data).unwrap();
        assert_eq!(
            decoded.max_packet_size,
            negotiator.capabilities.max_packet_size
        );
        assert_eq!(
            decoded.supported_types,
            negotiator.capabilities.supported_types
        );
    }
}
