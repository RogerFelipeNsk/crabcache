//! Protocol Negotiation for CrabCache
//! Automatically detects and negotiates the best protocol between client and server

use crate::protocol::protobuf::{ProtobufError, ProtobufResult, PROTOBUF_MAGIC, PROTOBUF_VERSION};
use bytes::{BufMut, Bytes, BytesMut};
use std::collections::HashMap;

/// Supported protocol types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProtocolType {
    /// Legacy text protocol (existing)
    Text,
    /// Native Protobuf protocol (Phase 8.1)
    Protobuf,
    /// Future TOON protocol (Phase 8.2)
    Toon,
}

impl ProtocolType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProtocolType::Text => "text",
            ProtocolType::Protobuf => "protobuf",
            ProtocolType::Toon => "toon",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "text" => Some(ProtocolType::Text),
            "protobuf" | "proto" => Some(ProtocolType::Protobuf),
            "toon" => Some(ProtocolType::Toon),
            _ => None,
        }
    }
}

/// Result of protocol negotiation
#[derive(Debug, Clone)]
pub struct NegotiationResult {
    pub protocol: ProtocolType,
    pub version: u8,
    pub capabilities: HashMap<String, String>,
    pub compression_enabled: bool,
    pub zero_copy_enabled: bool,
}

/// Protocol negotiator handles automatic protocol detection and negotiation
pub struct ProtocolNegotiator {
    supported_protocols: Vec<ProtocolType>,
    default_protocol: ProtocolType,
    capabilities: HashMap<String, String>,
}

impl ProtocolNegotiator {
    pub fn new() -> Self {
        let mut capabilities = HashMap::new();
        capabilities.insert("max_message_size".to_string(), "16777216".to_string()); // 16MB
        capabilities.insert("supports_compression".to_string(), "true".to_string());
        capabilities.insert("supports_zero_copy".to_string(), "true".to_string());
        capabilities.insert("supports_batch".to_string(), "true".to_string());
        capabilities.insert("supports_simd".to_string(), "true".to_string());

        Self {
            supported_protocols: vec![
                ProtocolType::Protobuf, // Preferred
                ProtocolType::Text,     // Fallback
            ],
            default_protocol: ProtocolType::Text, // Safe fallback
            capabilities,
        }
    }

    /// Detect protocol from incoming data
    pub fn detect_protocol(&self, data: &[u8]) -> ProtocolResult<ProtocolType> {
        if data.len() < 4 {
            return Ok(ProtocolType::Text); // Not enough data, assume text
        }

        // Check for Protobuf magic bytes
        if data[0..4] == PROTOBUF_MAGIC {
            return Ok(ProtocolType::Protobuf);
        }

        // Check for TOON magic (future)
        // if data[0..4] == TOON_MAGIC {
        //     return Ok(ProtocolType::Toon);
        // }

        // Default to text protocol
        Ok(ProtocolType::Text)
    }

    /// Negotiate protocol with client
    pub fn negotiate(&self, client_request: &[u8]) -> ProtobufResult<NegotiationResult> {
        let detected_protocol = self.detect_protocol(client_request)?;

        match detected_protocol {
            ProtocolType::Protobuf => {
                if client_request.len() < 6 {
                    return Err(ProtobufError::NegotiationFailed {
                        reason: "Insufficient data for Protobuf negotiation".to_string(),
                    });
                }

                // Verify magic bytes
                if client_request[0..4] != PROTOBUF_MAGIC {
                    return Err(ProtobufError::InvalidMagic([
                        client_request[0],
                        client_request[1],
                        client_request[2],
                        client_request[3],
                    ]));
                }

                // Check version
                let version = client_request[4];
                if version != PROTOBUF_VERSION {
                    return Err(ProtobufError::UnsupportedVersion(version));
                }

                Ok(NegotiationResult {
                    protocol: ProtocolType::Protobuf,
                    version,
                    capabilities: self.capabilities.clone(),
                    compression_enabled: true,
                    zero_copy_enabled: true,
                })
            }

            ProtocolType::Text => Ok(NegotiationResult {
                protocol: ProtocolType::Text,
                version: 1,
                capabilities: self.capabilities.clone(),
                compression_enabled: false,
                zero_copy_enabled: false,
            }),

            ProtocolType::Toon => Err(ProtobufError::NegotiationFailed {
                reason: "TOON protocol not yet implemented".to_string(),
            }),
        }
    }

    /// Create negotiation response
    pub fn create_negotiation_response(&self, result: &NegotiationResult) -> Bytes {
        match result.protocol {
            ProtocolType::Protobuf => {
                let mut response = BytesMut::with_capacity(64);

                // Magic bytes
                response.put_slice(&PROTOBUF_MAGIC);

                // Version
                response.put_u8(result.version);

                // Status (0 = success)
                response.put_u8(0);

                // Capabilities length (placeholder for now)
                response.put_u16(0);

                response.freeze()
            }

            ProtocolType::Text => Bytes::from_static(b"OK\n"),

            ProtocolType::Toon => Bytes::from_static(b"ERROR TOON not implemented\n"),
        }
    }

    /// Check if protocol is supported
    pub fn is_supported(&self, protocol: &ProtocolType) -> bool {
        self.supported_protocols.contains(protocol)
    }

    /// Get preferred protocol order
    pub fn get_preferred_protocols(&self) -> &[ProtocolType] {
        &self.supported_protocols
    }
}

impl Default for ProtocolNegotiator {
    fn default() -> Self {
        Self::new()
    }
}

// Type alias for convenience
type ProtocolResult<T> = Result<T, ProtobufError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_detection() {
        let negotiator = ProtocolNegotiator::new();

        // Test Protobuf detection
        let protobuf_data = [0x43, 0x52, 0x41, 0x42, 0x01, 0x00]; // CRAB + version
        assert_eq!(
            negotiator.detect_protocol(&protobuf_data).unwrap(),
            ProtocolType::Protobuf
        );

        // Test text detection
        let text_data = b"GET key\n";
        assert_eq!(
            negotiator.detect_protocol(text_data).unwrap(),
            ProtocolType::Text
        );

        // Test insufficient data
        let short_data = [0x43, 0x52];
        assert_eq!(
            negotiator.detect_protocol(&short_data).unwrap(),
            ProtocolType::Text
        );
    }

    #[test]
    fn test_protobuf_negotiation() {
        let negotiator = ProtocolNegotiator::new();

        // Valid Protobuf negotiation
        let valid_request = [0x43, 0x52, 0x41, 0x42, 0x01, 0x00];
        let result = negotiator.negotiate(&valid_request).unwrap();

        assert_eq!(result.protocol, ProtocolType::Protobuf);
        assert_eq!(result.version, 1);
        assert!(result.compression_enabled);
        assert!(result.zero_copy_enabled);
    }

    #[test]
    fn test_invalid_magic_bytes() {
        let negotiator = ProtocolNegotiator::new();

        let invalid_request = [0x00, 0x00, 0x00, 0x00, 0x01, 0x00];
        let result = negotiator.negotiate(&invalid_request);

        // Should fallback to text protocol
        assert!(result.is_ok());
        assert_eq!(result.unwrap().protocol, ProtocolType::Text);
    }

    #[test]
    fn test_unsupported_version() {
        let negotiator = ProtocolNegotiator::new();

        let unsupported_version = [0x43, 0x52, 0x41, 0x42, 0xFF, 0x00]; // Version 255
        let result = negotiator.negotiate(&unsupported_version);

        assert!(result.is_err());
        match result.unwrap_err() {
            ProtobufError::UnsupportedVersion(255) => {}
            _ => panic!("Expected UnsupportedVersion error"),
        }
    }
}
