//! Zero-copy command parser for ultra-low latency

use bytes::Bytes;
use std::str;

/// Zero-copy command reference (no allocations)
#[derive(Debug, Clone)]
pub enum CommandRef<'a> {
    Ping,
    Get {
        key: &'a [u8],
    },
    Put {
        key: &'a [u8],
        value: &'a [u8],
        ttl: Option<u64>,
    },
    Del {
        key: &'a [u8],
    },
    Expire {
        key: &'a [u8],
        ttl: u64,
    },
    Stats,
    Metrics,
}

impl<'a> CommandRef<'a> {
    /// Convert to owned command (only when necessary)
    pub fn to_owned(&self) -> crate::protocol::Command {
        match self {
            CommandRef::Ping => crate::protocol::Command::Ping,
            CommandRef::Get { key } => crate::protocol::Command::Get {
                key: Bytes::copy_from_slice(key),
            },
            CommandRef::Put { key, value, ttl } => crate::protocol::Command::Put {
                key: Bytes::copy_from_slice(key),
                value: Bytes::copy_from_slice(value),
                ttl: *ttl,
            },
            CommandRef::Del { key } => crate::protocol::Command::Del {
                key: Bytes::copy_from_slice(key),
            },
            CommandRef::Expire { key, ttl } => crate::protocol::Command::Expire {
                key: Bytes::copy_from_slice(key),
                ttl: *ttl,
            },
            CommandRef::Stats => crate::protocol::Command::Stats,
            CommandRef::Metrics => crate::protocol::Command::Metrics,
        }
    }
}

/// Ultra-fast zero-copy parser
pub struct ZeroCopyParser;

impl ZeroCopyParser {
    /// Parse command without any allocations
    #[inline(always)]
    pub fn parse_zero_copy(data: &[u8]) -> Result<CommandRef<'_>, &'static str> {
        if data.is_empty() {
            return Err("Empty command");
        }

        // Ultra-fast inline parsing for common cases
        match data.len() {
            4 if data == b"PING" => Ok(CommandRef::Ping),
            5 if data == b"STATS" => Ok(CommandRef::Stats),
            7 if data == b"METRICS" => Ok(CommandRef::Metrics),
            _ => Self::parse_with_args(data),
        }
    }

    /// Parse commands with arguments (zero-copy)
    #[inline(always)]
    fn parse_with_args(data: &[u8]) -> Result<CommandRef<'_>, &'static str> {
        if data.len() < 4 {
            return Err("Command too short");
        }

        // Check command prefix with SIMD-friendly comparisons
        match &data[0..4] {
            b"GET " => Self::parse_get(&data[4..]),
            b"PUT " => Self::parse_put(&data[4..]),
            b"DEL " => Self::parse_del(&data[4..]),
            _ => {
                // Check for longer commands
                if data.len() >= 7 && &data[0..7] == b"EXPIRE " {
                    Self::parse_expire(&data[7..])
                } else {
                    Err("Unknown command")
                }
            }
        }
    }

    /// Parse GET command (zero-copy)
    #[inline(always)]
    fn parse_get(args: &[u8]) -> Result<CommandRef<'_>, &'static str> {
        if args.is_empty() {
            return Err("GET missing key");
        }

        // Find key end (space or end of data)
        let key_end = args.iter().position(|&b| b == b' ').unwrap_or(args.len());

        if key_end == 0 {
            return Err("GET empty key");
        }

        Ok(CommandRef::Get {
            key: &args[..key_end],
        })
    }

    /// Parse PUT command (zero-copy)
    #[inline(always)]
    fn parse_put(args: &[u8]) -> Result<CommandRef<'_>, &'static str> {
        if args.is_empty() {
            return Err("PUT missing arguments");
        }

        // Find first space (key/value separator)
        let key_end = match args.iter().position(|&b| b == b' ') {
            Some(pos) => pos,
            None => return Err("PUT missing value"),
        };

        if key_end == 0 {
            return Err("PUT empty key");
        }

        let value_start = key_end + 1;
        if value_start >= args.len() {
            return Err("PUT missing value");
        }

        let key = &args[..key_end];

        // Find value end (next space or end of data)
        let value_end = args[value_start..]
            .iter()
            .position(|&b| b == b' ')
            .map(|pos| value_start + pos)
            .unwrap_or(args.len());

        let value = &args[value_start..value_end];

        // Check for TTL
        let ttl = if value_end < args.len() {
            let ttl_start = value_end + 1;
            if ttl_start < args.len() {
                let ttl_str =
                    str::from_utf8(&args[ttl_start..]).map_err(|_| "Invalid TTL encoding")?;
                Some(ttl_str.parse().map_err(|_| "Invalid TTL number")?)
            } else {
                None
            }
        } else {
            None
        };

        Ok(CommandRef::Put { key, value, ttl })
    }

    /// Parse DEL command (zero-copy)
    #[inline(always)]
    fn parse_del(args: &[u8]) -> Result<CommandRef<'_>, &'static str> {
        if args.is_empty() {
            return Err("DEL missing key");
        }

        // Find key end (space or end of data)
        let key_end = args.iter().position(|&b| b == b' ').unwrap_or(args.len());

        if key_end == 0 {
            return Err("DEL empty key");
        }

        Ok(CommandRef::Del {
            key: &args[..key_end],
        })
    }

    /// Parse EXPIRE command (zero-copy)
    #[inline(always)]
    fn parse_expire(args: &[u8]) -> Result<CommandRef<'_>, &'static str> {
        if args.is_empty() {
            return Err("EXPIRE missing arguments");
        }

        // Find first space (key/ttl separator)
        let key_end = match args.iter().position(|&b| b == b' ') {
            Some(pos) => pos,
            None => return Err("EXPIRE missing TTL"),
        };

        if key_end == 0 {
            return Err("EXPIRE empty key");
        }

        let ttl_start = key_end + 1;
        if ttl_start >= args.len() {
            return Err("EXPIRE missing TTL");
        }

        let key = &args[..key_end];
        let ttl_str = str::from_utf8(&args[ttl_start..]).map_err(|_| "Invalid TTL encoding")?;
        let ttl = ttl_str.parse().map_err(|_| "Invalid TTL number")?;

        Ok(CommandRef::Expire { key, ttl })
    }

    /// Parse batch of commands (zero-copy)
    pub fn parse_batch_zero_copy(data: &[u8]) -> Vec<Result<CommandRef<'_>, &'static str>> {
        let mut commands = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            if let Some(newline_pos) = data[offset..].iter().position(|&b| b == b'\n') {
                let command_end = offset + newline_pos;
                let command_data = &data[offset..command_end];

                // Remove \r if present
                let command_data = if command_data.ends_with(b"\r") {
                    &command_data[..command_data.len() - 1]
                } else {
                    command_data
                };

                if !command_data.is_empty() {
                    commands.push(Self::parse_zero_copy(command_data));
                }

                offset = command_end + 1;
            } else {
                // No more complete commands
                break;
            }
        }

        commands
    }

    /// Parse with SIMD optimization (when available)
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sse2")]
    pub unsafe fn parse_simd_optimized(data: &[u8]) -> Result<CommandRef<'_>, &'static str> {
        use std::arch::x86_64::*;

        if data.len() >= 16 {
            // Load first 16 bytes
            let chunk = _mm_loadu_si128(data.as_ptr() as *const __m128i);

            // Check for common commands using SIMD
            let ping_pattern = _mm_set_epi8(
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, b'G' as i8, b'N' as i8, b'I' as i8, b'P' as i8,
            );
            let ping_cmp = _mm_cmpeq_epi32(chunk, ping_pattern);
            let ping_mask = _mm_movemask_epi8(ping_cmp);

            if ping_mask & 0xF000 == 0xF000 && data.len() == 4 {
                return Ok(CommandRef::Ping);
            }
        }

        // Fallback to regular parsing
        Self::parse_zero_copy(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ping() {
        let result = ZeroCopyParser::parse_zero_copy(b"PING");
        assert!(matches!(result, Ok(CommandRef::Ping)));
    }

    #[test]
    fn test_parse_get() {
        let result = ZeroCopyParser::parse_zero_copy(b"GET mykey");
        match result {
            Ok(CommandRef::Get { key }) => {
                assert_eq!(key, b"mykey");
            }
            _ => panic!("Expected GET command"),
        }
    }

    #[test]
    fn test_parse_put() {
        let result = ZeroCopyParser::parse_zero_copy(b"PUT mykey myvalue");
        match result {
            Ok(CommandRef::Put { key, value, ttl }) => {
                assert_eq!(key, b"mykey");
                assert_eq!(value, b"myvalue");
                assert_eq!(ttl, None);
            }
            _ => panic!("Expected PUT command"),
        }
    }

    #[test]
    fn test_parse_put_with_ttl() {
        let result = ZeroCopyParser::parse_zero_copy(b"PUT mykey myvalue 3600");
        match result {
            Ok(CommandRef::Put { key, value, ttl }) => {
                assert_eq!(key, b"mykey");
                assert_eq!(value, b"myvalue");
                assert_eq!(ttl, Some(3600));
            }
            _ => panic!("Expected PUT command with TTL"),
        }
    }

    #[test]
    fn test_parse_batch() {
        let data = b"PING\r\nGET key1\r\nPUT key2 value2\r\n";
        let commands = ZeroCopyParser::parse_batch_zero_copy(data);

        assert_eq!(commands.len(), 3);
        assert!(matches!(commands[0], Ok(CommandRef::Ping)));
        assert!(matches!(commands[1], Ok(CommandRef::Get { .. })));
        assert!(matches!(commands[2], Ok(CommandRef::Put { .. })));
    }

    #[test]
    fn test_zero_copy_no_allocations() {
        let data = b"GET verylongkeyname";
        let result = ZeroCopyParser::parse_zero_copy(data);

        match result {
            Ok(CommandRef::Get { key }) => {
                // Verify that key points to original data (zero-copy)
                assert_eq!(key.as_ptr(), data[4..].as_ptr());
            }
            _ => panic!("Expected GET command"),
        }
    }
}
