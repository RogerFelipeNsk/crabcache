//! The keyspace: a fixed set of mutex-protected shards selected by key hash, with global memory
//! accounting for `maxmemory`.

pub mod chunked;
pub mod compress;
pub mod entry;
pub mod shard;

pub use compress::Codec;
pub use entry::Entry;
pub use shard::{PackCandidate, PackStats, Shard, ShardStats};

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
    /// CrabPack value compression (disabled until configured).
    pub codec: Codec,
    sample_rng: AtomicU64,
}

/// Position of the background compressor in the keyspace.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackCursor {
    shard: usize,
    pos: usize,
}

pub struct PackBudget {
    pub bytes: usize,
    pub max_value_bytes: usize,
    pub until: Instant,
}

#[derive(Default)]
pub struct PackWork {
    pub input_bytes: usize,
    pub scanned: usize,
    pub packed: usize,
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
            codec: Codec::new(false, 60, 64),
            sample_rng: AtomicU64::new(0x2545_F491_4F6C_DD1D),
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

    pub fn maintenance_clock_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
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
        let mut guard = self.shards[shard].0.lock();
        let base = guard.used();
        if self.codec.enabled() && !guard.tracks_access() {
            guard.enable_access_tracking(self.start.elapsed().as_secs() as u32);
        }
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

    /// Compression counters summed across shards.
    pub fn pack_summary(&self) -> PackStats {
        let mut total = PackStats::default();
        for i in 0..self.shard_count() {
            total += self.lock(i).pack_stats();
        }
        total
    }

    /// Samples up to `n` random values for dictionary training. Returns the prefixes that now have
    /// enough samples; the caller trains them with `codec.train` on a blocking thread.
    pub fn pack_sample(&self, n: usize) -> Vec<compress::SampleBatch> {
        let mut ready = Vec::new();
        if !self.codec.enabled() {
            return ready;
        }
        let clock = self.clock();
        let secs = clock.secs;
        let min_size = self.codec.min_size();
        for _ in 0..n {
            let r = self.sample_rng.fetch_add(0x9E37_79B9_7F4A_7C15, Relaxed);
            let shard = (r.rotate_left(17) as usize) % self.shard_count();
            let sample = {
                let mut g = self.lock(shard);
                g.random_index().and_then(|i| {
                    let e = g.entry(i);
                    let value = e.plain()?;
                    (value.len() >= min_size
                        && !e.is_expired(clock.ms)
                        && self.codec.ttl_eligible(e, clock.ms)
                        && self.codec.wants_samples(e.key(), secs))
                    .then(|| {
                        (
                            compress::prefix_of(e.key()).to_vec(),
                            self.codec.sample_fingerprint(e.key()),
                            value[..value.len().min(compress::MAX_SAMPLE_SIZE)].to_vec(),
                        )
                    })
                })
            };
            let batch = sample.and_then(|(prefix, key_hash, value)| {
                self.codec
                    .offer_sample_owned(&prefix, key_hash, value, secs)
            });
            ready.extend(batch);
            if !ready.is_empty() {
                break;
            }
        }
        ready
    }

    /// Byte-bounded snapshots and compression outside the lock. The deadline is cooperative:
    /// a single zstd call may finish after it, but its input is bounded by max_value_bytes.
    pub fn pack_step(&self, cursor: &mut PackCursor, scan: usize, budget: &PackBudget) -> PackWork {
        self.pack_step_at(cursor, scan, budget, self.clock())
    }

    fn pack_step_at(
        &self,
        cursor: &mut PackCursor,
        scan: usize,
        budget: &PackBudget,
        clock: Clock,
    ) -> PackWork {
        let mut work = PackWork::default();
        if !self.codec.enabled() || !self.codec.has_dicts() || Instant::now() >= budget.until {
            return work;
        }
        let min_idle = self.codec.min_idle_secs().min(u32::MAX as u64) as u32;
        let min_size = self.codec.min_size();
        let shard = cursor.shard % self.shard_count();
        let mut candidates = Vec::new();
        {
            let g = self.lock(shard);
            // Dense entries may have moved since the previous pass.
            cursor.pos = cursor.pos.min(g.len());
            if g.pack_stats().keys as usize == g.len() {
                cursor.shard = (shard + 1) % self.shard_count();
                cursor.pos = 0;
                return work;
            }
            let end = cursor.pos.saturating_add(scan).min(g.len());
            while cursor.pos < end && Instant::now() < budget.until {
                let i = cursor.pos;
                let e = g.entry(i);
                if let Some(value) = e.plain().filter(|v| v.len() >= min_size) {
                    if !self.codec.ttl_eligible(e, clock.ms) {
                        self.codec.stats.short_ttl_skips.fetch_add(1, Relaxed);
                        cursor.pos += 1;
                        work.scanned += 1;
                        continue;
                    }
                    let access_secs = g.last_access_secs(i);
                    let dict = (!e.is_expired(clock.ms)
                        && clock.secs.saturating_sub(access_secs) >= min_idle)
                        .then(|| self.codec.dict_for_key(e.key()))
                        .flatten();
                    if let Some(dict) = dict {
                        let bytes = e.key().len().saturating_add(value.len());
                        if bytes > budget.max_value_bytes {
                            self.codec.stats.oversized_skips.fetch_add(1, Relaxed);
                        } else if bytes > budget.bytes.saturating_sub(work.input_bytes) {
                            break; // Resume this candidate with a fresh budget next tick.
                        } else {
                            candidates.push(PackCandidate {
                                index: i,
                                key: e.key().to_vec(),
                                value: value.to_vec(),
                                access_secs,
                                dict: dict.id,
                            });
                            work.input_bytes += bytes;
                        }
                    }
                }
                cursor.pos += 1;
                work.scanned += 1;
                // Keep individual lock acquisitions small, even with many small values.
                if work.input_bytes >= 64 * 1024 {
                    break;
                }
            }
            if cursor.pos >= g.len() {
                cursor.shard = (shard + 1) % self.shard_count();
                cursor.pos = 0;
            }
        }
        for candidate in candidates {
            if Instant::now() >= budget.until {
                break;
            }
            let Some(dict) = self.codec.dict(candidate.dict) else {
                continue;
            };
            let Some(compressed) = self.codec.compress(dict, &candidate.value) else {
                continue;
            };
            let mut g = self.lock(shard);
            let clock = self.clock();
            if g.entries()
                .get(candidate.index)
                .is_some_and(|e| self.codec.ttl_eligible(e, clock.ms))
                && g.pack_at(&candidate, &compressed, clock, min_idle)
            {
                work.packed += 1;
            } else {
                self.codec.stats.candidates_discarded.fetch_add(1, Relaxed);
            }
        }
        self.codec
            .stats
            .values_packed
            .fetch_add(work.packed as u64, Relaxed);
        self.codec
            .stats
            .pack_input_bytes
            .fetch_add(work.input_bytes as u64, Relaxed);
        work
    }

    pub fn access_clock_bytes(&self) -> usize {
        (0..self.shard_count())
            .map(|i| self.lock(i).access_clock_bytes())
            .sum()
    }

    pub fn hot_resident_bytes(&self) -> usize {
        (0..self.shard_count())
            .map(|i| self.lock(i).hot_resident_bytes())
            .sum()
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

    pub fn wants_hot_promotion(&self, i: usize) -> bool {
        let budget = self.db.codec.hot_max_bytes();
        if budget == 0 || !self.db.codec.enabled() || self.policy() != Policy::AllKeysLfu {
            return false;
        }
        let limit = (budget / self.db.shard_count() as u64).min(usize::MAX as u64) as usize;
        self.shard.hot_promotion_growth(i, limit).is_some()
    }

    /// Reserve the growth globally before altering the entry, without eviction or changing a GET
    /// into an OOM response. Synchronize this guard's pending accounting so concurrent promotions
    /// cannot all spend the same headroom. The resident budget is partitioned across shards.
    pub fn promote_hot(&mut self, i: usize, value: &[u8], clock: Clock) -> bool {
        if value.len() != self.shard.entry(i).value_len() || !self.wants_hot_promotion(i) {
            return false;
        }
        let limit = (self.db.codec.hot_max_bytes() / self.db.shard_count() as u64)
            .min(usize::MAX as u64) as usize;
        let Some(growth) = self.shard.hot_promotion_growth(i, limit) else {
            return false;
        };
        let delta = self.shard.used() as i64 - self.base as i64;
        self.db.used.fetch_add(delta, Relaxed);
        self.base = self.shard.used();
        let max = self.db.maxmemory();
        let mut used = self.db.used.load(Relaxed);
        loop {
            let Some(next) = used.checked_add(growth as i64) else {
                return false;
            };
            if max != 0 && next.max(0) as u64 > max {
                return false;
            }
            match self
                .db
                .used
                .compare_exchange_weak(used, next, Relaxed, Relaxed)
            {
                Ok(_) => break,
                Err(actual) => used = actual,
            }
        }
        self.shard.promote_hot_at(i, value, clock);
        self.base = self.shard.used(); // Already charged by the reservation.
        self.db.codec.stats.hot_promotions.fetch_add(1, Relaxed);
        true
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
            Some(i) => g.set_value(i, v, clock.secs),
            None => {
                let meta = g.policy().initial_meta(clock);
                g.insert(h, Entry::new(k, v, 0, meta), clock.secs);
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
    #[test]
    fn access_clock_memory_is_lazy_accounted_and_retained_when_disabled() {
        let db = Db::new(4, 0, Policy::AllKeysLfu, 5);
        set(&db, b"k", b"value").unwrap();
        let plain = db.used_memory();
        assert_eq!(db.access_clock_bytes(), 0);
        db.codec.set_enabled(true);
        assert_eq!(db.access_clock_bytes(), 4);
        assert_eq!(db.used_memory(), plain + 4);
        db.codec.set_enabled(false);
        set(&db, b"other", b"value").unwrap();
        db.flush();
        assert_eq!((db.used_memory(), db.access_clock_bytes()), (0, 0));
    }

    fn trained_db() -> Db {
        let db = Db::new(1, 0, Policy::AllKeysLfu, 5);
        db.codec.set_enabled(true);
        db.codec.set_min_idle_secs(0);
        assert!(matches!(
            db.codec.train(
                b"session:".as_slice().into(),
                compress::tests::sample_values(1000, 9),
                0
            ),
            compress::TrainOutcome::Accepted { .. }
        ));
        for (i, v) in compress::tests::sample_values(50, 123).iter().enumerate() {
            set(&db, format!("session:{i}").as_bytes(), v).unwrap();
        }
        db
    }

    #[test]
    fn packing_respects_byte_budget_and_resumes_without_losing_candidates() {
        let db = trained_db();
        let mut cursor = PackCursor::default();
        let budget = PackBudget {
            bytes: 500,
            max_value_bytes: 500,
            until: Instant::now() + std::time::Duration::from_secs(10),
        };
        for _ in 0..100 {
            let work = db.pack_step(&mut cursor, 128, &budget);
            assert!(work.input_bytes <= budget.bytes);
            if db.pack_summary().keys == 50 {
                break;
            }
        }
        assert_eq!(db.pack_summary().keys, 50);
        for (i, expected) in compress::tests::sample_values(50, 123).iter().enumerate() {
            let (g, h) = db.lock_key(format!("session:{i}").as_bytes());
            let key = format!("session:{i}");
            let index = g
                .entries()
                .iter()
                .position(|e| e.key() == key.as_bytes())
                .unwrap();
            let _ = h;
            assert_eq!(
                db.codec.with_value(g.entry(index), |v| v.to_vec()).unwrap(),
                *expected
            );
        }
    }

    #[test]
    fn expired_budget_and_oversized_values_do_no_compression() {
        let db = trained_db();
        let mut cursor = PackCursor::default();
        let expired = PackBudget {
            bytes: 500,
            max_value_bytes: 500,
            until: Instant::now(),
        };
        assert_eq!(db.pack_step(&mut cursor, 128, &expired).input_bytes, 0);
        let tiny = PackBudget {
            bytes: 1,
            max_value_bytes: 1,
            until: Instant::now() + std::time::Duration::from_secs(1),
        };
        let work = db.pack_step(&mut cursor, 128, &tiny);
        assert_eq!((work.input_bytes, work.packed), (0, 0));
        assert_eq!(db.codec.stats.oversized_skips.load(Relaxed), 50);
        assert_eq!(db.pack_summary().keys, 0);
    }
}

#[cfg(test)]
mod hot_tests {
    use super::entry::Packed;
    use super::*;

    fn packed(db: &Db, key: &[u8], clock: Clock) {
        let (mut g, h) = db.lock_key(key);
        g.insert(
            h,
            Entry::new_packed(
                key,
                b"compressed",
                clock.ms + 60000,
                8,
                Packed {
                    dict: 0,
                    original_len: 512,
                },
            ),
            clock.secs,
        );
    }

    #[test]
    fn promotion_preserves_ttl_metadata_budget_and_dense_index_moves() {
        let db = Db::new(1, 0, Policy::AllKeysLfu, 5);
        db.codec.set_enabled(true);
        db.codec.set_hot_max_bytes(4096);
        let clock = db.clock();
        for k in [b"a", b"b"] {
            packed(&db, k, clock);
        }
        let before = db.used_memory();
        for k in [b"a", b"b"] {
            let (mut g, h) = db.lock_key(k);
            let i = g.lookup(h, k, clock.ms).unwrap();
            assert!(g.promote_hot(i, &[b'x'; 512], clock));
            assert_eq!(g.entry(i).expire_at(), clock.ms + 60000);
            assert_eq!(g.entry(i).meta, 8);
            assert!(g.entry(i).packed().is_none());
        }
        assert!(db.hot_resident_bytes() <= 4096);
        assert!(db.used_memory() > before);
        {
            let (mut g, h) = db.lock_key(b"a");
            let i = g.lookup(h, b"a", clock.ms).unwrap();
            g.remove_at(i, h); // Moves promoted b to index 0.
        }
        {
            let (mut g, h) = db.lock_key(b"b");
            let i = g.lookup(h, b"b", clock.ms).unwrap();
            assert_eq!(i, 0);
            g.set_value(i, b"replacement", clock.secs);
        }
        assert_eq!(db.hot_resident_bytes(), 0);
        db.flush();
        assert_eq!(db.used_memory(), 0);
    }

    #[test]
    fn promoted_resident_accounting_follows_expire_and_persist() {
        let db = Db::new(1, 0, Policy::AllKeysLfu, 5);
        db.codec.set_enabled(true);
        db.codec.set_hot_max_bytes(4096);
        let clock = db.clock();
        packed(&db, b"a", clock);
        let (mut g, h) = db.lock_key(b"a");
        let i = g.lookup(h, b"a", clock.ms).unwrap();
        g.set_expire(i, h, 0);
        assert!(g.promote_hot(i, &[b'x'; 512], clock));
        let before = g.hot_resident_bytes();
        let used = g.used();
        g.set_expire(i, h, clock.ms + 60000);
        assert_eq!(g.hot_resident_bytes(), before + 8);
        assert_eq!(g.used(), used + 8);
        g.set_expire(i, h, clock.ms + 120000);
        assert_eq!(g.hot_resident_bytes(), before + 8);
        g.set_expire(i, h, 0);
        assert_eq!(g.hot_resident_bytes(), before);
        assert_eq!(g.used(), used);
        assert_eq!(g.entry(i).plain(), Some([b'x'; 512].as_slice()));
        assert_eq!(g.entry(i).meta, 8);
    }

    #[test]
    fn promotion_budget_reserves_space_for_a_future_ttl() {
        let db = Db::new(1, 0, Policy::AllKeysLfu, 5);
        db.codec.set_enabled(true);
        let clock = db.clock();
        packed(&db, b"a", clock);
        let plain = Entry::new(b"a", &[b'x'; 512], 0, 8).mem_usage();
        let metadata = 64 * std::mem::size_of::<(usize, usize)>();
        db.codec.set_hot_max_bytes((plain + metadata) as u64);
        let (mut g, h) = db.lock_key(b"a");
        let i = g.lookup(h, b"a", clock.ms).unwrap();
        g.set_expire(i, h, 0);
        assert!(!g.promote_hot(i, &[b'x'; 512], clock));
        db.codec.set_hot_max_bytes((plain + metadata + 8) as u64);
        assert!(g.promote_hot(i, &[b'x'; 512], clock));
        g.set_expire(i, h, clock.ms + 60000);
        assert_eq!(g.hot_resident_bytes() as u64, db.codec.hot_max_bytes());
    }

    #[test]
    fn promotion_cannot_evict_or_exceed_maxmemory_or_resident_budget() {
        let db = Db::new(1, 0, Policy::AllKeysLfu, 5);
        db.codec.set_enabled(true);
        db.codec.set_hot_max_bytes(4096);
        let clock = db.clock();
        packed(&db, b"a", clock);
        let before = db.used_memory();
        db.set_maxmemory(before);
        {
            let (mut g, h) = db.lock_key(b"a");
            let i = g.lookup(h, b"a", clock.ms).unwrap();
            assert!(!g.promote_hot(i, &[b'x'; 512], clock));
            assert!(g.entry(i).packed().is_some());
        }
        assert_eq!(db.used_memory(), before);
        assert_eq!(db.dbsize(), 1);
        db.set_maxmemory(0);
        db.codec.set_hot_max_bytes(100); // Less than value + tracking.
        let (mut g, h) = db.lock_key(b"a");
        let i = g.lookup(h, b"a", clock.ms).unwrap();
        assert!(!g.promote_hot(i, &[b'x'; 512], clock));
    }
    #[test]
    fn concurrent_promotions_share_one_maxmemory_reservation() {
        let db = Db::new(2, 0, Policy::AllKeysLfu, 5);
        db.codec.set_enabled(true);
        db.codec.set_hot_max_bytes(8192);
        let a = b"a".to_vec();
        let b = (0..10000)
            .map(|i| format!("b{i}").into_bytes())
            .find(|k| db.shard_index(db.hash(k)) != db.shard_index(db.hash(&a)))
            .unwrap();
        let clock = db.clock();
        packed(&db, &a, clock);
        packed(&db, &b, clock);
        let growth = {
            let (mut g, h) = db.lock_key(&a);
            let i = g.lookup(h, &a, clock.ms).unwrap();
            g.hot_promotion_growth(i, 4096).unwrap()
        };
        db.set_maxmemory(db.used_memory() + growth as u64);
        let barrier = std::sync::Barrier::new(2);
        let results = std::thread::scope(|scope| {
            let workers: Vec<_> = [&a, &b]
                .into_iter()
                .map(|key| {
                    let db = &db;
                    let barrier = &barrier;
                    scope.spawn(move || {
                        let (mut g, h) = db.lock_key(key);
                        let i = g.lookup(h, key, clock.ms).unwrap();
                        barrier.wait();
                        g.promote_hot(i, &[b'x'; 512], clock)
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|w| w.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().filter(|&&ok| ok).count(), 1);
        assert!(db.used_memory() <= db.maxmemory());
        assert_eq!(db.dbsize(), 2);
    }
}
