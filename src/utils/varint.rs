//! High-performance variable-length integer encoding/decoding

use bytes::{Buf, BufMut, Bytes, BytesMut};

/// Encode a u64 as a varint into a BytesMut buffer (optimized)
pub fn encode_varint(value: u64, buf: &mut BytesMut) {
    let mut val = value;
    while val >= 0x80 {
        buf.put_u8((val & 0x7F) as u8 | 0x80);
        val >>= 7;
    }
    buf.put_u8(val as u8);
}

/// Decode a varint from a byte slice, returning (value, bytes_consumed)
pub fn decode_varint(bytes: &[u8]) -> crate::Result<(u64, usize)> {
    let mut result = 0u64;
    let mut shift = 0;

    for (i, &byte) in bytes.iter().enumerate().take(10) {
        // Max 10 bytes for u64
        if shift >= 64 {
            return Err("Varint overflow".into());
        }

        result |= ((byte & 0x7F) as u64) << shift;

        if byte & 0x80 == 0 {
            return Ok((result, i + 1));
        }

        shift += 7;

        if i == 9 {
            return Err("Varint too long".into());
        }
    }

    Err("Incomplete varint".into())
}

/// Legacy decode function for backward compatibility
pub fn decode_varint_bytes(buf: &mut Bytes) -> Option<u64> {
    let mut result = 0u64;
    let mut shift = 0;

    for i in 0..10 {
        // Max 10 bytes for u64
        if buf.is_empty() {
            return None;
        }

        let byte = buf.get_u8();

        if shift >= 64 {
            return None; // Overflow
        }

        result |= ((byte & 0x7F) as u64) << shift;

        if byte & 0x80 == 0 {
            return Some(result);
        }

        shift += 7;

        if i == 9 {
            return None; // Too many bytes
        }
    }

    None
}

/// Legacy encode function for backward compatibility
pub fn encode_varint_bytes(buf: &mut BytesMut, mut value: u64) {
    while value >= 0x80 {
        buf.put_u8((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }
    buf.put_u8(value as u8);
}

/// Calculate the size of a varint encoding (optimized)
#[inline]
pub fn varint_size(value: u64) -> usize {
    if value == 0 {
        return 1;
    }

    // Use bit manipulation for faster calculation
    let leading_zeros = value.leading_zeros();
    let bits_needed = 64 - leading_zeros;
    ((bits_needed + 6) / 7) as usize // Ceiling division by 7
}

/// Fast varint encoding for small values (0-127) - single byte
#[inline]
pub fn encode_varint_small(value: u8, buf: &mut BytesMut) {
    debug_assert!(value < 0x80);
    buf.put_u8(value);
}

/// Fast varint decoding for single byte values
#[inline]
pub fn try_decode_varint_small(byte: u8) -> Option<u64> {
    if byte & 0x80 == 0 {
        Some(byte as u64)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip() {
        let test_values = vec![0, 1, 127, 128, 255, 256, 65535, 65536, u64::MAX];

        for value in test_values {
            let mut buf = BytesMut::new();
            encode_varint(value, &mut buf);

            let bytes = buf.freeze();
            let (decoded, consumed) = decode_varint(&bytes).unwrap();

            assert_eq!(value, decoded, "Failed for value {}", value);
            assert_eq!(
                consumed,
                bytes.len(),
                "Consumed bytes mismatch for value {}",
                value
            );
        }
    }

    #[test]
    fn test_varint_size_optimized() {
        assert_eq!(varint_size(0), 1);
        assert_eq!(varint_size(127), 1);
        assert_eq!(varint_size(128), 2);
        assert_eq!(varint_size(16383), 2);
        assert_eq!(varint_size(16384), 3);
        assert_eq!(varint_size(u64::MAX), 10);
    }

    #[test]
    fn test_small_varint_optimization() {
        let mut buf = BytesMut::new();
        encode_varint_small(42, &mut buf);

        let bytes = buf.freeze();
        assert_eq!(bytes.len(), 1);
        assert_eq!(try_decode_varint_small(bytes[0]), Some(42));
    }

    #[test]
    fn test_backward_compatibility() {
        let test_values = vec![0, 1, 127, 128, 255, 256, 65535, 65536];

        for value in test_values {
            // Test old vs new encode
            let mut buf1 = BytesMut::new();
            let mut buf2 = BytesMut::new();

            encode_varint_bytes(&mut buf1, value);
            encode_varint(value, &mut buf2);

            let bytes1 = buf1.freeze();
            let bytes2 = buf2.freeze();

            assert_eq!(bytes1, bytes2, "Encode mismatch for {}", value);

            // Test old vs new decode
            let mut bytes1_copy = bytes1.clone();
            let old_result = decode_varint_bytes(&mut bytes1_copy);
            let (new_result, _) = decode_varint(&bytes2).unwrap();

            assert_eq!(
                old_result,
                Some(new_result),
                "Decode mismatch for {}",
                value
            );
        }
    }
}
