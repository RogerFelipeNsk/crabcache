//! Hash functions for key distribution

use ahash::AHasher;
use std::hash::{Hash, Hasher};

/// Hash a key for shard distribution
pub fn hash_key(key: &[u8]) -> u64 {
    let mut hasher = AHasher::default();
    key.hash(&mut hasher);
    hasher.finish()
}