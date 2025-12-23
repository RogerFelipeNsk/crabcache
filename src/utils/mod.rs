//! Utility functions and helpers

pub mod hash;
pub mod simd;
pub mod varint;

pub use hash::hash_key;
pub use simd::{SIMDParser, SIMDMetrics, CPUFeatures};
pub use varint::{encode_varint, decode_varint};