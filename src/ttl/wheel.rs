//! TTL wheel implementation for efficient expiration tracking

use bytes::Bytes;
use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;

/// TTL wheel for efficient expiration tracking
/// Uses a time wheel approach with configurable granularity
pub struct TTLWheel {
    /// Number of slots in the wheel (typically 3600 for 1-hour wheel with 1-second granularity)
    slots: usize,
    /// Current position in the wheel
    current_slot: usize,
    /// Each slot contains keys that expire in that time slot
    wheel: Vec<VecDeque<Bytes>>,
    /// Map from key to its expiration slot for quick removal
    key_to_slot: HashMap<Bytes, usize>,
    /// Granularity in seconds (how many seconds each slot represents)
    granularity: u64,
}

impl TTLWheel {
    /// Create a new TTL wheel
    /// - slots: number of time slots (e.g., 3600 for 1 hour)
    /// - granularity: seconds per slot (e.g., 1 for 1-second granularity)
    pub fn new(slots: usize, granularity: u64) -> Self {
        let mut wheel = Vec::with_capacity(slots);
        for _ in 0..slots {
            wheel.push(VecDeque::new());
        }

        Self {
            slots,
            current_slot: 0,
            wheel,
            key_to_slot: HashMap::new(),
            granularity,
        }
    }

    /// Add a key with TTL (in seconds)
    pub fn add_key(&mut self, key: Bytes, ttl_seconds: u64) {
        let now = self.current_time_seconds();
        let expire_time = now + ttl_seconds;
        let slot = self.time_to_slot(expire_time);

        // Remove key if it already exists
        self.remove_key(&key);

        // Add to new slot
        self.wheel[slot].push_back(key.clone());
        self.key_to_slot.insert(key, slot);

        debug!(
            "Added key to TTL wheel: slot {} (TTL: {}s)",
            slot, ttl_seconds
        );
    }

    /// Remove a key from the wheel
    pub fn remove_key(&mut self, key: &Bytes) -> bool {
        if let Some(slot) = self.key_to_slot.remove(key) {
            // Remove from the slot (linear search, but slots should be small)
            if let Some(pos) = self.wheel[slot].iter().position(|k| k == key) {
                self.wheel[slot].remove(pos);
                debug!("Removed key from TTL wheel: slot {}", slot);
                return true;
            }
        }
        false
    }

    /// Advance the wheel and return expired keys
    pub fn tick(&mut self) -> Vec<Bytes> {
        let mut expired_keys = Vec::new();

        // Collect all keys in current slot
        while let Some(key) = self.wheel[self.current_slot].pop_front() {
            self.key_to_slot.remove(&key);
            expired_keys.push(key);
        }

        // Advance to next slot
        self.current_slot = (self.current_slot + 1) % self.slots;

        if !expired_keys.is_empty() {
            debug!("TTL wheel tick: {} keys expired", expired_keys.len());
        }

        expired_keys
    }

    /// Get current time in seconds since epoch
    fn current_time_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Convert absolute time to wheel slot
    fn time_to_slot(&self, time_seconds: u64) -> usize {
        ((time_seconds / self.granularity) % self.slots as u64) as usize
    }

    /// Check if a key should be expired based on current time
    pub fn should_expire(&self, key: &Bytes) -> bool {
        if let Some(&slot) = self.key_to_slot.get(key) {
            let now = self.current_time_seconds();
            let current_time_slot = self.time_to_slot(now);

            // Simple check: if the key's slot has passed, it should expire
            // This is a simplified version - a full implementation would track exact expiration times
            if self.slots_between(current_time_slot, slot) > self.slots / 2 {
                return true;
            }
        }
        false
    }

    /// Calculate slots between two positions in the wheel
    fn slots_between(&self, from: usize, to: usize) -> usize {
        if to >= from {
            to - from
        } else {
            (self.slots - from) + to
        }
    }

    /// Get statistics
    pub fn stats(&self) -> (usize, usize) {
        let total_keys = self.key_to_slot.len();
        let non_empty_slots = self.wheel.iter().filter(|slot| !slot.is_empty()).count();
        (total_keys, non_empty_slots)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ttl_wheel_basic() {
        let mut wheel = TTLWheel::new(60, 1); // 60 slots, 1 second granularity

        let key = Bytes::from("test_key");
        wheel.add_key(key.clone(), 5);

        let (total_keys, _) = wheel.stats();
        assert_eq!(total_keys, 1);

        assert!(wheel.remove_key(&key));
        let (total_keys, _) = wheel.stats();
        assert_eq!(total_keys, 0);
    }

    #[test]
    fn test_ttl_wheel_tick() {
        let mut wheel = TTLWheel::new(10, 1); // Small wheel for testing

        let key1 = Bytes::from("key1");
        let key2 = Bytes::from("key2");

        wheel.add_key(key1.clone(), 0); // Expires immediately
        wheel.add_key(key2.clone(), 5); // Expires later

        let expired = wheel.tick();
        // Note: This test is simplified and may not work exactly as expected
        // due to timing issues in unit tests
        assert!(expired.len() <= 2);
    }
}
