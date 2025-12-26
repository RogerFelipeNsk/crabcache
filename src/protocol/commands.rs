//! Command and response definitions

use bytes::Bytes;

/// Commands supported by CrabCache
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// PUT key value [ttl]
    Put {
        key: Bytes,
        value: Bytes,
        ttl: Option<u64>,
    },
    /// GET key
    Get { key: Bytes },
    /// DEL key
    Del { key: Bytes },
    /// EXPIRE key ttl
    Expire { key: Bytes, ttl: u64 },
    /// STATS
    Stats,
    /// METRICS - Performance metrics
    Metrics,
    /// PING
    Ping,
}

/// Response types
#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    /// OK response
    Ok,
    /// Value response
    Value(Bytes),
    /// Null response (key not found)
    Null,
    /// Error response
    Error(String),
    /// Pong response
    Pong,
    /// Stats response
    Stats(String),
}

/// Serializable version of Response for distributed operations
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SerializableResponse {
    /// OK response
    Ok,
    /// Value response (as Vec<u8> for serialization)
    Value(Vec<u8>),
    /// Null response (key not found)
    Null,
    /// Error response
    Error(String),
    /// Pong response
    Pong,
    /// Stats response
    Stats(String),
}

impl From<Response> for SerializableResponse {
    fn from(response: Response) -> Self {
        match response {
            Response::Ok => SerializableResponse::Ok,
            Response::Value(bytes) => SerializableResponse::Value(bytes.to_vec()),
            Response::Null => SerializableResponse::Null,
            Response::Error(msg) => SerializableResponse::Error(msg),
            Response::Pong => SerializableResponse::Pong,
            Response::Stats(stats) => SerializableResponse::Stats(stats),
        }
    }
}

impl From<SerializableResponse> for Response {
    fn from(response: SerializableResponse) -> Self {
        match response {
            SerializableResponse::Ok => Response::Ok,
            SerializableResponse::Value(vec) => Response::Value(Bytes::from(vec)),
            SerializableResponse::Null => Response::Null,
            SerializableResponse::Error(msg) => Response::Error(msg),
            SerializableResponse::Pong => Response::Pong,
            SerializableResponse::Stats(stats) => Response::Stats(stats),
        }
    }
}
