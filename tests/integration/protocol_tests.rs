//! Protocol integration tests

use bytes::Bytes;
use crabcache::protocol::commands::{Command, Response};
use crabcache::protocol::{ProtocolParser, ProtocolSerializer};

#[tokio::test]
async fn test_ping_command() {
    let command_bytes = b"PING\r\n";
    let command = ProtocolParser::parse_command(command_bytes).unwrap();
    assert_eq!(command, Command::Ping);

    let response = Response::Pong;
    let response_bytes = ProtocolSerializer::serialize_response(&response).unwrap();
    assert_eq!(response_bytes, Bytes::from("PONG\r\n"));
}

#[tokio::test]
async fn test_put_command() {
    let command_bytes = b"PUT mykey myvalue\r\n";
    let command = ProtocolParser::parse_command(command_bytes).unwrap();

    match command {
        Command::Put { key, value, ttl } => {
            assert_eq!(key, Bytes::from("mykey"));
            assert_eq!(value, Bytes::from("myvalue"));
            assert_eq!(ttl, None);
        }
        _ => panic!("Expected PUT command"),
    }
}

#[tokio::test]
async fn test_put_command_with_ttl() {
    let command_bytes = b"PUT mykey myvalue 3600\r\n";
    let command = ProtocolParser::parse_command(command_bytes).unwrap();

    match command {
        Command::Put { key, value, ttl } => {
            assert_eq!(key, Bytes::from("mykey"));
            assert_eq!(value, Bytes::from("myvalue"));
            assert_eq!(ttl, Some(3600));
        }
        _ => panic!("Expected PUT command"),
    }
}

#[tokio::test]
async fn test_get_command() {
    let command_bytes = b"GET mykey\r\n";
    let command = ProtocolParser::parse_command(command_bytes).unwrap();

    match command {
        Command::Get { key } => {
            assert_eq!(key, Bytes::from("mykey"));
        }
        _ => panic!("Expected GET command"),
    }
}

#[tokio::test]
async fn test_del_command() {
    let command_bytes = b"DEL mykey\r\n";
    let command = ProtocolParser::parse_command(command_bytes).unwrap();

    match command {
        Command::Del { key } => {
            assert_eq!(key, Bytes::from("mykey"));
        }
        _ => panic!("Expected DEL command"),
    }
}

#[tokio::test]
async fn test_expire_command() {
    let command_bytes = b"EXPIRE mykey 3600\r\n";
    let command = ProtocolParser::parse_command(command_bytes).unwrap();

    match command {
        Command::Expire { key, ttl } => {
            assert_eq!(key, Bytes::from("mykey"));
            assert_eq!(ttl, 3600);
        }
        _ => panic!("Expected EXPIRE command"),
    }
}

#[tokio::test]
async fn test_stats_command() {
    let command_bytes = b"STATS\r\n";
    let command = ProtocolParser::parse_command(command_bytes).unwrap();
    assert_eq!(command, Command::Stats);
}

#[tokio::test]
async fn test_response_serialization() {
    let responses = vec![
        (Response::Ok, "OK\r\n"),
        (Response::Pong, "PONG\r\n"),
        (Response::Null, "NULL\r\n"),
        (
            Response::Error("Test error".to_string()),
            "ERROR: Test error\r\n",
        ),
        (Response::Value(Bytes::from("test_value")), "test_value\r\n"),
        (
            Response::Stats("test stats".to_string()),
            "STATS: test stats\r\n",
        ),
    ];

    for (response, expected) in responses {
        let serialized = ProtocolSerializer::serialize_response(&response).unwrap();
        assert_eq!(serialized, Bytes::from(expected));
    }
}

#[tokio::test]
async fn test_invalid_command() {
    let command_bytes = b"INVALID\r\n";
    let result = ProtocolParser::parse_command(command_bytes);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_incomplete_command() {
    let command_bytes = b"PUT\r\n";
    let result = ProtocolParser::parse_command(command_bytes);
    assert!(result.is_err());
}
