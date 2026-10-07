//! The keyspace: a fixed set of mutex-protected shards selected by key hash, with global memory
//! accounting for `maxmemory`.

pub mod chunked;
pub mod entry;
pub mod shard;

pub use entry::Entry;
pub use shard::{Shard, ShardStats};

use ahash::RandomState;
use parking_lot::{Mutex, MutexGuard};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicI64, AtomicU8, AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;

/// Time snapshot taken once per batch of commands.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    /// Unix time in milliseconds (expiry deadlines).
    pub ms: u64,
    /// Seconds since server start (LRU/LFU clocks).
    pub secs: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Policy {
    NoEviction = 0,
    AllKeysLru = 1,
    AllKeysLfu = 2,
    AllKeysRandom = 3,
}

impl Policy {
    pub fn parse(s: &[u8]) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_slice() {
            b"noeviction" => Self::NoEviction,
            b"allkeys-lru" => Self::AllKeysLru,
            b"allkeys-lfu" => Self::AllKeysLfu,
            b"allkeys-random" => Self::AllKeysRandom,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::NoEviction => "noeviction",
            Self::AllKeysLru => "allkeys-lru",
            Self::AllKeysLfu => "allkeys-lfu",
            Self::AllKeysRandom => "allkeys-random",
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::AllKeysLru,
            2 => Self::AllKeysLfu,
            3 => Self::AllKeysRandom,
            _ => Self::NoEviction,
        }
    }
}

/// Write rejected because `maxmemory` is reached under `noeviction`.
#[derive(Debug)]
pub struct OutOfMemory;

/// Keeps each shard's mutex on its own cache line (128 bytes covers Apple silicon and x86 prefetch pairs).
#[repr(align(128))]
struct Padded<T>(T);

pub struct Db {
    shards: Box<[Padded<Mutex<Shard>>]>,
    hasher: RandomState,
    mask: u64,
    used: AtomicI64,
    maxmemory: AtomicU64,
    policy: AtomicU8,
    samples: AtomicUsize,
    start: Instant,
    /// Wakes the background evictor when a writer could not free enough memory in its own shard.
    pub evict_notify: Notify,
}

impl Db {
    /// `shards` is rounded up to a power of two.
    pub fn new(shards: usize, maxmemory: u64, policy: Policy, samples: usize) -> Self {
        let n = shards.clamp(1, 1 << 16).next_power_of_two();
        let hasher = RandomState::new();
        let shards = (0..n)
            .map(|i| {
                Padded(Mutex::new(Shard::new(
                    hasher.clone(),
                    0x9E37_79B9_7F4A_7C15 ^ (i as u64 + 1),
                )))
            })
            .collect();
        Self {
            shards,
            hasher,
            mask: (n - 1) as u64,
            used: AtomicI64::new(0),
            maxmemory: AtomicU64::new(maxmemory),
            policy: AtomicU8::new(policy as u8),
            samples: AtomicUsize::new(samples),
            start: Instant::now(),
            evict_notify: Notify::new(),
        }
    }

    pub fn clock(&self) -> Clock {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Clock {
            ms,
            secs: self.start.elapsed().as_secs() as u32,
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start.elapsed().as_secs()
    }

    pub fn hash(&self, key: &[u8]) -> u64 {
        self.hasher.hash_one(key)
    }

    /// Shard owning a hash. Uses bits 40.. so it stays independent of the bits hashbrown uses for its
    /// bucket index (low bits) and control tags (top 7 bits).
    pub fn shard_index(&self, hash: u64) -> usize {
        ((hash >> 40) & self.mask) as usize
    }

    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    pub fn lock(&self, shard: usize) -> ShardGuard<'_> {
        let guard = self.shards[shard].0.lock();
        let base = guard.used();
        ShardGuard {
            db: self,
            shard: guard,
            base,
        }
    }

    /// Locks the shard owning `key` and returns it with the key hash.
    pub fn lock_key(&self, key: &[u8]) -> (ShardGuard<'_>, u64) {
        let h = self.hash(key);
        (self.lock(self.shard_index(h)), h)
    }

    /// Locks several shards in ascending order (deadlock-free). `shards` must be sorted and unique.
    pub fn lock_many(&self, shards: &[usize]) -> Vec<ShardGuard<'_>> {
        debug_assert!(shards.windows(2).all(|w| w[0] < w[1]));
        shards.iter().map(|&i| self.lock(i)).collect()
    }

    pub fn used_memory(&self) -> u64 {
        self.used.load(Relaxed).max(0) as u64
    }

    pub fn maxmemory(&self) -> u64 {
        self.maxmemory.load(Relaxed)
    }

    pub fn set_maxmemory(&self, v: u64) {
        self.maxmemory.store(v, Relaxed);
        self.evict_notify.notify_one();
    }

    pub fn policy(&self) -> Policy {
        Policy::from_u8(self.policy.load(Relaxed))
    }

    pub fn set_policy(&self, p: Policy) {
        self.policy.store(p as u8, Relaxed);
    }

    pub fn samples(&self) -> usize {
        self.samples.load(Relaxed)
    }

    pub fn set_samples(&self, v: usize) {
        self.samples.store(v.clamp(1, 64), Relaxed);
    }

    /// Key count, keys with expiry, and summed stats across all shards.
    pub fn summary(&self) -> (usize, usize, ShardStats) {
        let (mut keys, mut ttl, mut stats) = (0, 0, ShardStats::default());
        for i in 0..self.shard_count() {
            let g = self.lock(i);
            keys += g.len();
            ttl += g.ttl_keys();
            stats += g.stats;
        }
        (keys, ttl, stats)
    }

    pub fn dbsize(&self) -> usize {
        (0..self.shard_count()).map(|i| self.lock(i).len()).sum()
    }

    pub fn flush(&self) {
        for i in 0..self.shard_count() {
            self.lock(i).clear();
        }
    }

    pub fn reset_stats(&self) {
        for i in 0..self.shard_count() {
            self.lock(i).stats = ShardStats::default();
        }
    }

    /// One active-expiration pass over all shards. Returns true if some shard hit its budget.
    pub fn expire_cycle(&self, budget_per_shard: usize) -> bool {
        let now = self.clock().ms;
        let mut more = false;
        for i in 0..self.shard_count() {
            let mut g = self.lock(i);
            more |= g.active_expire(now, budget_per_shard) == budget_per_shard;
            g.maintain();
        }
        more
    }

    /// Evicts across shards until under `maxmemory` or `max_evictions` is reached.
    pub fn evict_cycle(&self, start_shard: &mut usize, max_evictions: usize) -> usize {
        let max = self.maxmemory();
        let policy = self.policy();
        if max == 0 || policy == Policy::NoEviction {
            return 0;
        }
        let clock = self.clock();
        let samples = self.samples();
        let mut evicted = 0;
        let mut empty_streak = 0;
        while self.used_memory() > max
            && evicted < max_evictions
            && empty_streak < self.shard_count()
        {
            let i = *start_shard % self.shard_count();
            *start_shard = start_shard.wrapping_add(1);
            let mut g = self.lock(i);
            if g.evict_one(policy, samples, clock) {
                evicted += 1;
                empty_streak = 0;
            } else {
                empty_streak += 1;
            }
        }
        evicted
    }
}

/// A locked shard. On drop, the change in the shard's memory usage is published to the global counter.
pub struct ShardGuard<'a> {
    db: &'a Db,
    shard: MutexGuard<'a, Shard>,
    base: usize,
}

impl ShardGuard<'_> {
    /// Makes room for a write of about `incoming` bytes, evicting from this shard if needed.
    pub fn admit(&mut self, incoming: usize, clock: Clock) -> Result<(), OutOfMemory> {
        let max = self.db.maxmemory();
        if max == 0 {
            return Ok(());
        }
        let policy = self.db.policy();
        if policy == Policy::NoEviction {
            return if self.effective_used() > max as i64 {
                Err(OutOfMemory)
            } else {
                Ok(())
            };
        }
        let samples = self.db.samples();
        let mut evicted = 0;
        while self.effective_used() + incoming as i64 > max as i64 {
            if evicted == 64 || !self.shard.evict_one(policy, samples, clock) {
                // This shard alone cannot cover the overage; let the background evictor rebalance.
                self.db.evict_notify.notify_one();
                break;
            }
            evicted += 1;
        }
        Ok(())
    }

    fn effective_used(&self) -> i64 {
        self.db.used.load(Relaxed) + self.shard.used() as i64 - self.base as i64
    }

    pub fn policy(&self) -> Policy {
        self.db.policy()
    }
}

impl Deref for ShardGuard<'_> {
    type Target = Shard;
    fn deref(&self) -> &Shard {
        &self.shard
    }
}

impl DerefMut for ShardGuard<'_> {
    fn deref_mut(&mut self) -> &mut Shard {
        &mut self.shard
    }
}

impl Drop for ShardGuard<'_> {
    fn drop(&mut self) {
        let delta = self.shard.used() as i64 - self.base as i64;
        if delta != 0 {
            self.db.used.fetch_add(delta, Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(db: &Db, k: &[u8], v: &[u8]) -> Result<(), OutOfMemory> {
        let clock = db.clock();
        let (mut g, h) = db.lock_key(k);
        g.admit(k.len() + v.len() + entry::ENTRY_OVERHEAD, clock)?;
        match g.lookup(h, k, clock.ms) {
            Some(i) => g.set_value(i, v),
            None => {
                let meta = g.policy().initial_meta(clock);
                g.insert(h, Entry::new(k, v, 0, meta));
            }
        }
        Ok(())
    }

    #[test]
    fn memory_accounting_tracks_writes_and_flush() {
        let db = Db::new(16, 0, Policy::NoEviction, 5);
        for i in 0..1000 {
            set(&db, format!("key{i}").as_bytes(), &[b'x'; 100]).unwrap();
        }
        let expected: u64 = (0..1000)
            .map(|i| {
                Entry::new(format!("key{i}").as_bytes(), &[b'x'; 100], 0, 0).mem_usage() as u64
            })
            .sum();
        assert_eq!(db.used_memory(), expected);
        assert_eq!(db.dbsize(), 1000);
        db.flush();
        assert_eq!(db.used_memory(), 0);
    }

    #[test]
    fn noeviction_rejects_writes_over_limit() {
        let db = Db::new(4, 50_000, Policy::NoEviction, 5);
        let mut rejected = 0;
        for i in 0..1000 {
            if set(&db, format!("key{i}").as_bytes(), &[b'x'; 100]).is_err() {
                rejected += 1;
            }
        }
        assert!(rejected > 0);
        assert!(db.used_memory() <= 50_000 + 200);
    }

    #[test]
    fn eviction_keeps_memory_near_limit() {
        for policy in [
            Policy::AllKeysLru,
            Policy::AllKeysLfu,
            Policy::AllKeysRandom,
        ] {
            let db = Db::new(8, 200_000, policy, 5);
            for i in 0..20_000 {
                set(&db, format!("key{i}").as_bytes(), &[b'x'; 100]).unwrap();
            }
            let mut cursor = 0;
            db.evict_cycle(&mut cursor, usize::MAX);
            assert!(
                db.used_memory() <= 200_000,
                "{policy:?}: used {}",
                db.used_memory()
            );
            let (keys, _, stats) = db.summary();
            assert!(
                keys > 500 && stats.evicted > 15_000,
                "{policy:?}: keys {keys} evicted {}",
                stats.evicted
            );
        }
    }
}
