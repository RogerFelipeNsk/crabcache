//! Protobuf Serializer for CrabCache
//! High-performance serializer with zero-copy optimizations

use bytes::{BufMut, Bytes, BytesMut};
use std::collections::HashMap;
use std::time::Instant;

use crate::protocol::protobuf::{
    generated::{
        crab_cache_response, BatchResponse, CrabCacheResponse, ErrorResponse, NullResponse,
        OkResponse, PongResponse, ProtocolCapabilities, ResponseStatus, ServerInfo, StatsResponse,
        ValueResponse,
    },
    ProtobufConfig, ProtobufMetrics, ProtobufResult, PROTOBUF_MAGIC, PROTOBUF_VERSION,
};
use crate::protocol::Response;

/// High-performance Protobuf serializer
pub struct ProtobufSerializer {
    config: ProtobufConfig,
    metrics: ProtobufMetrics,
    zero_copy_enabled: bool,
}

impl ProtobufSerializer {
    pub fn new(config: ProtobufConfig) -> Self {
        Self {
            zero_copy_enabled: config.enable_zero_copy,
            config,
            metrics: ProtobufMetrics::default(),
        }
    }

    /// Serialize a response to Protobuf format
    pub fn serialize_response(
        &mut self,
        response: Response,
        request_id: String,
    ) -> ProtobufResult<Bytes> {
        let start_time = Instant::now();

        // Convert internal response to protobuf response
        let proto_response = self.convert_response_to_proto(response, request_id)?;

        // Encode the message
        let mut buf = Vec::new();
        proto_response.encode(&mut buf).map_err(|_| {
            crate::protocol::protobuf::ProtobufError::DecodeError(prost::DecodeError::new(
                "Stub encode error",
            ))
        })?;

        // Create final message with protocol header
        let final_message = self.create_message_with_header(buf)?;

        // Update metrics
        let serialize_time = start_time.elapsed().as_micros() as f64;
        self.metrics
            .update_message_processed(final_message.len(), 0.0, serialize_time);

        Ok(final_message)
    }

    /// Serialize multiple responses as a batch
    pub fn serialize_batch_response(
        &mut self,
        responses: Vec<Response>,
        request_ids: Vec<String>,
    ) -> ProtobufResult<Bytes> {
        let start_time = Instant::now();

        if responses.len() != request_ids.len() {
            return Err(crate::protocol::protobuf::ProtobufError::DecodeError(
                prost::DecodeError::new("Response and request ID count mismatch"),
            ));
        }

        let mut proto_responses = Vec::with_capacity(responses.len());
        let mut successful_count = 0u32;
        let mut failed_count = 0u32;

        for (response, request_id) in responses.into_iter().zip(request_ids.into_iter()) {
            match &response {
                Response::Error(_) => failed_count += 1,
                _ => successful_count += 1,
            }

            let proto_response = self.convert_response_to_proto(response, request_id)?;
            proto_responses.push(proto_response);
        }

        // Create batch response
        let batch_response = BatchResponse {
            responses: proto_responses,
            successful_count,
            failed_count,
        };

        let main_response = CrabCacheResponse {
            request_id: "batch".to_string(),
            timestamp: chrono::Utc::now().timestamp() as u64,
            status: ResponseStatus::Success,
            response: Some(crab_cache_response::Response::Batch(batch_response)),
        };

        // Encode the message
        let mut buf = Vec::new();
        main_response.encode(&mut buf).map_err(|_| {
            crate::protocol::protobuf::ProtobufError::DecodeError(prost::DecodeError::new(
                "Stub encode error",
            ))
        })?;

        // Create final message with protocol header
        let final_message = self.create_message_with_header(buf)?;

        // Update metrics
        let serialize_time = start_time.elapsed().as_micros() as f64;
        self.metrics
            .update_message_processed(final_message.len(), 0.0, serialize_time);

        Ok(final_message)
    }

    /// Convert internal response to protobuf response
    fn convert_response_to_proto(
        &self,
        response: Response,
        request_id: String,
    ) -> ProtobufResult<CrabCacheResponse> {
        let timestamp = chrono::Utc::now().timestamp() as u64;

        let (status, proto_response) = match response {
            Response::Ok => (
                ResponseStatus::Success,
                Some(crab_cache_response::Response::Ok(OkResponse {
                    message: None,
                })),
            ),

            Response::Value(bytes) => (
                ResponseStatus::Success,
                Some(crab_cache_response::Response::Value(ValueResponse {
                    value: bytes,
                    metadata: HashMap::new(),
                    ttl_remaining: None,
                })),
            ),

            Response::Null => (
                ResponseStatus::NotFound,
                Some(crab_cache_response::Response::Null(NullResponse {
                    reason: "Key not found".to_string(),
                })),
            ),

            Response::Error(msg) => (
                ResponseStatus::Error,
                Some(crab_cache_response::Response::Error(ErrorResponse {
                    error_code: "GENERIC_ERROR".to_string(),
                    message: msg,
                    details: None,
                })),
            ),

            Response::Pong => (
                ResponseStatus::Success,
                Some(crab_cache_response::Response::Pong(PongResponse {
                    message: None,
                    server_timestamp: timestamp,
                })),
            ),

            Response::Stats(stats_json) => (
                ResponseStatus::Success,
                Some(crab_cache_response::Response::Stats(StatsResponse {
                    stats: self.parse_stats_json(stats_json)?,
                    server_info: self.create_server_info(),
                })),
            ),
        };

        Ok(CrabCacheResponse {
            request_id,
            timestamp,
            status,
            response: proto_response,
        })
    }

    /// Create message with protocol header
    fn create_message_with_header(&self, message_data: Vec<u8>) -> ProtobufResult<Bytes> {
        let total_size = 4 + 1 + 4 + message_data.len(); // magic + version + length + data
        let mut buf = BytesMut::with_capacity(total_size);

        // Protocol magic bytes
        buf.put_slice(&PROTOBUF_MAGIC);

        // Protocol version
        buf.put_u8(PROTOBUF_VERSION);

        // Message length (big-endian)
        buf.put_u32(message_data.len() as u32);

        // Message data
        buf.put_slice(&message_data);

        Ok(buf.freeze())
    }

    /// Parse stats JSON into HashMap (simplified for now)
    fn parse_stats_json(&self, stats_json: String) -> ProtobufResult<HashMap<String, String>> {
        // For now, just return the JSON as a single entry
        // In the future, we could parse the JSON and extract individual metrics
        let mut stats = HashMap::new();
        stats.insert("json_data".to_string(), stats_json);
        Ok(stats)
    }

    /// Create server info
    fn create_server_info(&self) -> ServerInfo {
        ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: 0,      // TODO: Track actual uptime
            total_connections: 0,   // TODO: Track from metrics
            current_connections: 0, // TODO: Track from metrics
            capabilities: ProtocolCapabilities {
                supports_batch: true,
                supports_ttl: true,
                supports_metadata: true,
                supports_compression: self.config.enable_compression,
                supports_zero_copy: self.config.enable_zero_copy,
                supports_simd: true,
                max_batch_size: 128, // From advanced pipeline
                max_value_size: self.config.max_message_size as u64,
            },
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

impl Default for ProtobufSerializer {
    fn default() -> Self {
        Self::new(ProtobufConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn test_serialize_ok_response() {
        let mut serializer = ProtobufSerializer::default();

        let response = Response::Ok;
        let request_id = "test_123".to_string();

        let serialized = serializer.serialize_response(response, request_id).unwrap();

        // Check that it starts with magic bytes
        assert_eq!(&serialized[0..4], &PROTOBUF_MAGIC);
        assert_eq!(serialized[4], PROTOBUF_VERSION);

        // Should have length field and data
        assert!(serialized.len() > 9); // magic + version + length + some data
    }

    #[test]
    fn test_serialize_value_response() {
        let mut serializer = ProtobufSerializer::default();

        let test_value = Bytes::from("test_value_data");
        let response = Response::Value(test_value.clone());
        let request_id = "test_456".to_string();

        let serialized = serializer.serialize_response(response, request_id).unwrap();

        // Verify protocol header
        assert_eq!(&serialized[0..4], &PROTOBUF_MAGIC);
        assert_eq!(serialized[4], PROTOBUF_VERSION);

        // Extract message length
        let message_len =
            u32::from_be_bytes([serialized[5], serialized[6], serialized[7], serialized[8]])
                as usize;

        // Verify total length
        assert_eq!(serialized.len(), 9 + message_len);

        // Decode the protobuf message to verify content
        let message_data = &serialized[9..];
        let decoded = CrabCacheResponse::decode(message_data).unwrap();

        assert_eq!(decoded.request_id, "test_456");
        assert_eq!(decoded.status, ResponseStatus::Success.into());

        match decoded.response {
            Some(crab_cache_response::Response::Value(value_response)) => {
                assert_eq!(value_response.value, test_value.to_vec());
            }
            _ => panic!("Expected Value response"),
        }
    }

    #[test]
    fn test_serialize_error_response() {
        let mut serializer = ProtobufSerializer::default();

        let error_msg = "Test error message".to_string();
        let response = Response::Error(error_msg.clone());
        let request_id = "test_error".to_string();

        let serialized = serializer.serialize_response(response, request_id).unwrap();

        // Decode and verify
        let message_data = &serialized[9..];
        let decoded = CrabCacheResponse::decode(message_data).unwrap();

        assert_eq!(decoded.status, ResponseStatus::Error.into());

        match decoded.response {
            Some(crab_cache_response::Response::Error(error_response)) => {
                assert_eq!(error_response.message, error_msg);
                assert_eq!(error_response.error_code, "GENERIC_ERROR");
            }
            _ => panic!("Expected Error response"),
        }
    }

    #[test]
    fn test_serialize_batch_response() {
        let mut serializer = ProtobufSerializer::default();

        let responses = vec![
            Response::Ok,
            Response::Value(Bytes::from("test_value")),
            Response::Error("test_error".to_string()),
        ];

        let request_ids = vec![
            "req_1".to_string(),
            "req_2".to_string(),
            "req_3".to_string(),
        ];

        let serialized = serializer
            .serialize_batch_response(responses, request_ids)
            .unwrap();

        // Decode and verify
        let message_data = &serialized[9..];
        let decoded = CrabCacheResponse::decode(message_data).unwrap();

        match decoded.response {
            Some(crab_cache_response::Response::Batch(batch_response)) => {
                assert_eq!(batch_response.responses.len(), 3);
                assert_eq!(batch_response.successful_count, 2);
                assert_eq!(batch_response.failed_count, 1);
            }
            _ => panic!("Expected Batch response"),
        }
    }

    #[test]
    fn test_protocol_header_format() {
        let mut serializer = ProtobufSerializer::default();

        let response = Response::Pong;
        let serialized = serializer
            .serialize_response(response, "ping_test".to_string())
            .unwrap();

        // Verify exact header format
        assert_eq!(serialized[0], 0x43); // 'C'
        assert_eq!(serialized[1], 0x52); // 'R'
        assert_eq!(serialized[2], 0x41); // 'A'
        assert_eq!(serialized[3], 0x42); // 'B'
        assert_eq!(serialized[4], 0x01); // Version 1

        // Length should be in big-endian format
        let expected_length = serialized.len() - 9;
        let actual_length =
            u32::from_be_bytes([serialized[5], serialized[6], serialized[7], serialized[8]])
                as usize;

        assert_eq!(actual_length, expected_length);
    }
}
