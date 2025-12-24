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
