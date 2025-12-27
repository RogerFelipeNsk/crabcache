//! Integration tests for Protobuf support
//! Phase 8.1 - Testing the revolutionary Protobuf native support

use bytes::Bytes;
use crabcache::protocol::{
    ProtobufConfig, ProtobufParser, ProtobufSerializer, ProtocolNegotiator, ProtocolType, Response,
    PROTOBUF_MAGIC, PROTOBUF_VERSION,
};
use prost::Message;

#[tokio::test]
async fn test_protocol_negotiation_flow() {
    let negotiator = ProtocolNegotiator::new();

    // Test Protobuf protocol detection
    let protobuf_request = [0x43, 0x52, 0x41, 0x42, 0x01, 0x00];
    let detected = negotiator.detect_protocol(&protobuf_request).unwrap();
    assert_eq!(detected, ProtocolType::Protobuf);

    // Test negotiation
    let result = negotiator.negotiate(&protobuf_request).unwrap();
    assert_eq!(result.protocol, ProtocolType::Protobuf);
    assert_eq!(result.version, 1);
    assert!(result.zero_copy_enabled);
    assert!(result.compression_enabled);

    // Test response creation
    let response = negotiator.create_negotiation_response(&result);
    assert_eq!(&response[0..4], &PROTOBUF_MAGIC);
    assert_eq!(response[4], PROTOBUF_VERSION);
}

#[tokio::test]
async fn test_text_protocol_fallback() {
    let negotiator = ProtocolNegotiator::new();

    // Test text protocol detection
    let text_request = b"GET key\n";
    let detected = negotiator.detect_protocol(text_request).unwrap();
    assert_eq!(detected, ProtocolType::Text);

    // Test negotiation fallback
    let result = negotiator.negotiate(text_request).unwrap();
    assert_eq!(result.protocol, ProtocolType::Text);
    assert!(!result.zero_copy_enabled);
    assert!(!result.compression_enabled);
}

#[tokio::test]
async fn test_protobuf_serialization_roundtrip() {
    let config = ProtobufConfig::default();
    let mut serializer = ProtobufSerializer::new(config);

    // Test different response types
    let test_cases = vec![
        ("ok", Response::Ok),
        ("value", Response::Value(Bytes::from("test_value"))),
        ("null", Response::Null),
        ("error", Response::Error("test error".to_string())),
        ("pong", Response::Pong),
    ];

    for (name, response) in test_cases {
        let request_id = format!("test_{}", name);
        let serialized = serializer
            .serialize_response(response, request_id.clone())
            .unwrap();

        // Verify protocol header
        assert_eq!(&serialized[0..4], &PROTOBUF_MAGIC);
        assert_eq!(serialized[4], PROTOBUF_VERSION);

        // Verify message length field
        let message_len =
            u32::from_be_bytes([serialized[5], serialized[6], serialized[7], serialized[8]])
                as usize;

        assert_eq!(serialized.len(), 9 + message_len);

        // Verify we can decode the protobuf message
        let message_data = &serialized[9..];
        let decoded =
            crabcache::protocol::protobuf::generated::CrabCacheResponse::decode(message_data);
        assert!(decoded.is_ok(), "Failed to decode {} response", name);

        let decoded = decoded.unwrap();
        assert_eq!(decoded.request_id, request_id);
    }
}

#[tokio::test]
async fn test_batch_serialization() {
    let config = ProtobufConfig::default();
    let mut serializer = ProtobufSerializer::new(config);

    let responses = vec![
        Response::Ok,
        Response::Value(Bytes::from("batch_value")),
        Response::Error("batch_error".to_string()),
    ];

    let request_ids = vec![
        "batch_1".to_string(),
        "batch_2".to_string(),
        "batch_3".to_string(),
    ];

    let serialized = serializer
        .serialize_batch_response(responses, request_ids)
        .unwrap();

    // Verify protocol header
    assert_eq!(&serialized[0..4], &PROTOBUF_MAGIC);
    assert_eq!(serialized[4], PROTOBUF_VERSION);

    // Decode and verify batch structure
    let message_data = &serialized[9..];
    let decoded =
        crabcache::protocol::protobuf::generated::CrabCacheResponse::decode(message_data).unwrap();

    match decoded.response {
        Some(crabcache::protocol::protobuf::generated::crab_cache_response::Response::Batch(
            batch,
        )) => {
            assert_eq!(batch.responses.len(), 3);
            assert_eq!(batch.successful_count, 2);
            assert_eq!(batch.failed_count, 1);
        }
        _ => panic!("Expected batch response"),
    }
}

#[tokio::test]
async fn test_protocol_metrics() {
    let config = ProtobufConfig::default();
    let mut serializer = ProtobufSerializer::new(config);

    // Process several messages to generate metrics
    for i in 0..10 {
        let response = Response::Value(Bytes::from(format!("value_{}", i)));
        let request_id = format!("req_{}", i);
        let _serialized = serializer.serialize_response(response, request_id).unwrap();
    }

    let metrics = serializer.get_metrics();
    assert_eq!(metrics.messages_processed, 10);
    assert!(metrics.avg_serialize_time_us > 0.0);
    assert!(metrics.avg_message_size > 0.0);
}

#[tokio::test]
async fn test_invalid_protocol_version() {
    let negotiator = ProtocolNegotiator::new();

    // Test unsupported version
    let invalid_request = [0x43, 0x52, 0x41, 0x42, 0xFF, 0x00]; // Version 255
    let result = negotiator.negotiate(&invalid_request);

    assert!(result.is_err());
    match result.unwrap_err() {
        crabcache::protocol::protobuf::ProtobufError::UnsupportedVersion(255) => {}
        _ => panic!("Expected UnsupportedVersion error"),
    }
}

#[tokio::test]
async fn test_message_size_limits() {
    let config = ProtobufConfig {
        max_message_size: 100, // Very small limit for testing
        ..Default::default()
    };

    let mut parser = ProtobufParser::new(config);

    // Create a message larger than the limit
    let large_data = vec![0u8; 200];
    let data = Bytes::from(large_data);

    let result = parser.parse_command(data);
    assert!(result.is_err());

    match result.unwrap_err() {
        crabcache::protocol::protobuf::ProtobufError::MessageTooLarge {
            size: 200,
            max: 100,
        } => {}
        _ => panic!("Expected MessageTooLarge error"),
    }
}

#[tokio::test]
async fn test_zero_copy_configuration() {
    let config = ProtobufConfig {
        enable_zero_copy: true,
        ..Default::default()
    };

    let parser = ProtobufParser::new(config.clone());
    let serializer = ProtobufSerializer::new(config);

    assert!(parser.is_zero_copy_enabled());
    assert!(serializer.is_zero_copy_enabled());
}

#[tokio::test]
async fn test_compression_configuration() {
    let config = ProtobufConfig {
        enable_compression: true,
        compression_threshold: 1024,
        ..Default::default()
    };

    // Test that configuration is properly stored
    assert!(config.enable_compression);
    assert_eq!(config.compression_threshold, 1024);
}

#[tokio::test]
async fn test_buffer_pool_integration() {
    use crabcache::protocol::protobuf::ProtobufBufferPool;

    let pool = ProtobufBufferPool::default();

    // Get a buffer
    let buf1 = pool.get_buffer();
    assert!(buf1.capacity() > 0);

    // Return it
    pool.return_buffer(buf1);

    // Get another buffer (should reuse the returned one)
    let buf2 = pool.get_buffer();
    assert!(buf2.capacity() > 0);

    // Check stats
    let stats = pool.stats();
    assert_eq!(stats.max_size, 1000);
    assert_eq!(stats.default_capacity, 4096);
}

#[tokio::test]
async fn test_schema_registry_basic() {
    use crabcache::protocol::protobuf::SchemaRegistry;

    let registry = SchemaRegistry::default();

    // Register a schema
    let schema_data = b"test schema data".to_vec();
    registry
        .register_schema("test_schema".to_string(), schema_data.clone())
        .unwrap();

    // Retrieve the schema
    let retrieved = registry.get_schema("test_schema");
    assert_eq!(retrieved, Some(schema_data));

    // Check existence
    assert!(registry.has_schema("test_schema"));
    assert!(!registry.has_schema("nonexistent_schema"));

    // Check count
    assert_eq!(registry.schema_count(), 1);
}
