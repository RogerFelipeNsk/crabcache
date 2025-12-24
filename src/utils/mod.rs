//! Utility functions and helpers

pub mod hash;
pub mod simd;
pub mod varint;

pub use hash::hash_key;
pub use simd::{CPUFeatures, SIMDMetrics, SIMDParser};
pub use varint::{decode_varint, encode_varint};
