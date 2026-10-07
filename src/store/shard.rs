//! One shard of the keyspace: a dense entry array indexed by a hash table, plus a min-heap of expiry
//! deadlines for active expiration.
//!
//! Entries live in a dense chunked array so a uniformly random entry can be picked in O(1); that is
//! what sampled LRU/LFU eviction needs (the same approach Redis uses). Deletion is a `swap_remove`,
//! with the moved entry's index slot patched.

use super::chunked::Chunked;
use super::entry::Entry;
use super::{Clock, Policy};
use ahash::RandomState;
use hashbrown::HashTable;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const LFU_INIT_VAL: u32 = 5;
const LFU_LOG_FACTOR: f64 = 10.0;
const LFU_DECAY_MINUTES: u32 = 1;

#[derive(Default, Clone, Copy, Debug)]
pub struct ShardStats {
    pub hits: u64,
    pub misses: u64,
    pub expired: u64,
    pub evicted: u64,
}

impl std::ops::AddAssign for ShardStats {
    fn add_assign(&mut self, o: Self) {
        self.hits += o.hits;
        self.misses += o.misses;
        self.expired += o.expired;
        self.evicted += o.evicted;
    }
}

pub struct Shard {
    entries: Chunked<Entry>,
    index: HashTable<u32>,
    /// `(deadline_ms, key_hash)`. May hold stale deadlines; they are skipped when popped.
    expiries: BinaryHeap<Reverse<(u64, u64)>>,
    ttl_keys: usize,
    used: usize,
    hasher: RandomState,
    rng: u64,
    pub stats: ShardStats,
}

impl Shard {
    pub fn new(hasher: RandomState, seed: u64) -> Self {
        Self {
            entries: Chunked::default(),
            index: HashTable::new(),
            expiries: BinaryHeap::new(),
            ttl_keys: 0,
            used: 0,
            hasher,
            rng: seed | 1,
            stats: ShardStats::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Estimated memory used by the entries of this shard.
    pub fn used(&self) -> usize {
        self.used
    }

    pub fn ttl_keys(&self) -> usize {
        self.ttl_keys
    }

    pub fn entries(&self) -> &Chunked<Entry> {
        &self.entries
    }

    pub fn entry(&self, i: usize) -> &Entry {
        &self.entries[i]
    }

    fn find(&self, hash: u64, key: &[u8]) -> Option<usize> {
        let entries = &self.entries;
        self.index
            .find(hash, |&i| entries[i as usize].key() == key)
            .map(|&i| i as usize)
    }

    /// Index of the live entry for `key`. An expired entry is deleted and reported as missing.
    pub fn lookup(&mut self, hash: u64, key: &[u8], now_ms: u64) -> Option<usize> {
        let i = self.find(hash, key)?;
        if self.entries[i].is_expired(now_ms) {
            self.remove_at(i, hash);
            self.stats.expired += 1;
            return None;
        }
        Some(i)
    }

    /// Records a read access for hit/miss stats and eviction metadata.
    pub fn touch(&mut self, i: usize, policy: Policy, clock: Clock) {
        let meta = self.entries[i].meta;
        self.entries[i].meta = match policy {
            Policy::AllKeysLfu => {
                let r = self.next_f64();
                let counter = lfu_incr(lfu_decayed(meta, clock), r);
                (lfu_minutes(clock) << 8) | counter
            }
            _ => clock.secs,
        };
    }

    /// Inserts an entry whose key is known to be absent. Returns its index.
    pub fn insert(&mut self, hash: u64, entry: Entry) -> usize {
        let i = self.entries.len();
        let expire_at = entry.expire_at();
        self.used += entry.mem_usage();
        self.entries.push(entry);
        let (entries, hasher) = (&self.entries, &self.hasher);
        self.index.insert_unique(hash, i as u32, |&j| {
            hasher.hash_one(entries[j as usize].key())
        });
        if expire_at != 0 {
            self.ttl_keys += 1;
            self.push_expiry(expire_at, hash);
        }
        i
    }

    /// Replaces entry `i` (same key) with a prebuilt entry, so the allocation can happen outside the lock.
    pub fn replace(&mut self, i: usize, hash: u64, entry: Entry) -> Entry {
        let (old_at, new_at) = (self.entries[i].expire_at(), entry.expire_at());
        match (old_at != 0, new_at != 0) {
            (false, true) => self.ttl_keys += 1,
            (true, false) => self.ttl_keys -= 1,
            _ => {}
        }
        self.used += entry.mem_usage();
        let old = std::mem::replace(&mut self.entries[i], entry);
        self.used -= old.mem_usage();
        if new_at != 0 && new_at != old_at {
            self.push_expiry(new_at, hash);
        }
        old
    }

    pub fn set_value(&mut self, i: usize, value: &[u8]) {
        self.used -= self.entries[i].mem_usage();
        self.entries[i].set_value(value);
        self.used += self.entries[i].mem_usage();
    }

    /// Sets (`at > 0`) or clears (`at == 0`) the expiry of entry `i`.
    pub fn set_expire(&mut self, i: usize, hash: u64, at: u64) {
        let old = self.entries[i].expire_at();
        match (old != 0, at != 0) {
            (false, true) => self.ttl_keys += 1,
            (true, false) => self.ttl_keys -= 1,
            _ => {}
        }
        // Adding or removing an expiry changes the allocation size.
        self.used -= self.entries[i].mem_usage();
        self.entries[i].set_expire_at(at);
        self.used += self.entries[i].mem_usage();
        if at != 0 && at != old {
            self.push_expiry(at, hash);
        }
    }

    pub fn remove_at(&mut self, i: usize, hash: u64) -> Entry {
        let last = self.entries.len() - 1;
        match self.index.find_entry(hash, |&j| j as usize == i) {
            Ok(slot) => {
                slot.remove();
            }
            Err(_) => unreachable!("entry {i} missing from index"),
        }
        if i != last {
            let moved = self.hasher.hash_one(self.entries[last].key());
            if let Some(slot) = self.index.find_mut(moved, |&j| j as usize == last) {
                *slot = i as u32;
            }
        }
        let e = self.entries.swap_remove(i);
        self.used -= e.mem_usage();
        if e.expire_at() != 0 {
            self.ttl_keys -= 1;
        }
        e
    }

    pub fn hash_of(&self, key: &[u8]) -> u64 {
        self.hasher.hash_one(key)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.index = HashTable::new();
        self.expiries = BinaryHeap::new();
        self.ttl_keys = 0;
        self.used = 0;
    }

    pub fn random_index(&mut self) -> Option<usize> {
        if self.entries.is_empty() {
            None
        } else {
            Some((self.next_u64() % self.entries.len() as u64) as usize)
        }
    }

    /// Deletes up to `budget` keys whose deadline has passed. Returns the number of heap items processed.
    pub fn active_expire(&mut self, now_ms: u64, budget: usize) -> usize {
        let mut processed = 0;
        while processed < budget {
            match self.expiries.peek() {
                Some(Reverse((t, _))) if *t <= now_ms => {}
                _ => break,
            }
            let Reverse((t, h)) = self.expiries.pop().unwrap();
            let entries = &self.entries;
            if let Some(&i) = self
                .index
                .find(h, |&j| entries[j as usize].expire_at() == t)
            {
                self.remove_at(i as usize, h);
                self.stats.expired += 1;
            }
            processed += 1;
        }
        processed
    }

    /// Evicts one entry chosen among `samples` random candidates. Returns false if the shard is empty.
    pub fn evict_one(&mut self, policy: Policy, samples: usize, clock: Clock) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        let mut victim = 0;
        let mut best = u64::MAX;
        for _ in 0..samples.max(1) {
            let i = (self.next_u64() % self.entries.len() as u64) as usize;
            let score = match policy {
                Policy::AllKeysLfu => lfu_decayed(self.entries[i].meta, clock) as u64,
                Policy::AllKeysLru => self.entries[i].meta as u64,
                _ => 0,
            };
            if score < best {
                best = score;
                victim = i;
            }
        }
        let h = self.hash_of(self.entries[victim].key());
        self.remove_at(victim, h);
        self.stats.evicted += 1;
        true
    }

    /// Returns memory to the allocator after large deletions.
    pub fn maintain(&mut self) {
        // Entry chunks are freed as the array shrinks; only the hash index keeps its peak capacity.
        let len = self.entries.len();
        if self.index.capacity() > 4 * len + 1024 {
            let (entries, hasher) = (&self.entries, &self.hasher);
            self.index
                .shrink_to(2 * len, |&j| hasher.hash_one(entries[j as usize].key()));
        }
    }

    fn push_expiry(&mut self, at: u64, hash: u64) {
        self.expiries.push(Reverse((at, hash)));
        if self.expiries.len() > 2 * self.ttl_keys + 1024 {
            let hasher = &self.hasher;
            self.expiries = self
                .entries
                .iter()
                .filter(|e| e.expire_at() != 0)
                .map(|e| Reverse((e.expire_at(), hasher.hash_one(e.key()))))
                .collect();
        }
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl Policy {
    /// Eviction metadata for a newly written entry.
    pub fn initial_meta(self, clock: Clock) -> u32 {
        match self {
            Policy::AllKeysLfu => (lfu_minutes(clock) << 8) | LFU_INIT_VAL,
            _ => clock.secs,
        }
    }
}

fn lfu_minutes(clock: Clock) -> u32 {
    (clock.secs / 60) & 0xFFFF
}

/// LFU counter after applying time decay (Redis `LFUDecrAndReturn`).
fn lfu_decayed(meta: u32, clock: Clock) -> u32 {
    let ldt = meta >> 8;
    let counter = meta & 0xFF;
    let now = lfu_minutes(clock);
    let elapsed = if now >= ldt {
        now - ldt
    } else {
        65535 - ldt + now
    };
    counter.saturating_sub(elapsed / LFU_DECAY_MINUTES)
}

/// Logarithmic counter increment (Redis `LFULogIncr`).
fn lfu_incr(counter: u32, r: f64) -> u32 {
    if counter >= 255 {
        return 255;
    }
    let base = counter.saturating_sub(LFU_INIT_VAL) as f64;
    if r < 1.0 / (base * LFU_LOG_FACTOR + 1.0) {
        counter + 1
    } else {
        counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shard() -> Shard {
        Shard::new(RandomState::with_seeds(1, 2, 3, 4), 42)
    }

    fn clock(ms: u64) -> Clock {
        Clock {
            ms,
            secs: (ms / 1000) as u32,
        }
    }

    fn put(s: &mut Shard, k: &str, v: &str, exp: u64) {
        let h = s.hash_of(k.as_bytes());
        match s.lookup(h, k.as_bytes(), 0) {
            Some(i) => {
                s.set_value(i, v.as_bytes());
                s.set_expire(i, h, exp);
            }
            None => {
                s.insert(h, Entry::new(k.as_bytes(), v.as_bytes(), exp, 0));
            }
        }
    }

    fn get(s: &mut Shard, k: &str, now: u64) -> Option<String> {
        let h = s.hash_of(k.as_bytes());
        s.lookup(h, k.as_bytes(), now)
            .map(|i| String::from_utf8(s.entry(i).value().to_vec()).unwrap())
    }

    #[test]
    fn insert_overwrite_remove_keep_index_consistent() {
        let mut s = shard();
        for i in 0..1000 {
            put(&mut s, &format!("k{i}"), &format!("v{i}"), 0);
        }
        put(&mut s, "k5", "new", 0);
        assert_eq!(get(&mut s, "k5", 0).as_deref(), Some("new"));
        // Remove every third key; swap_remove must keep the moved entries reachable.
        for i in (0..1000).step_by(3) {
            let k = format!("k{i}");
            let h = s.hash_of(k.as_bytes());
            let idx = s.lookup(h, k.as_bytes(), 0).unwrap();
            s.remove_at(idx, h);
        }
        for i in 0..1000 {
            let got = get(&mut s, &format!("k{i}"), 0);
            if i % 3 == 0 {
                assert_eq!(got, None);
            } else if i == 5 {
                assert_eq!(got.as_deref(), Some("new"));
            } else {
                assert_eq!(got, Some(format!("v{i}")));
            }
        }
        assert_eq!(s.len(), 666);
        let expected: usize = s.entries().iter().map(|e| e.mem_usage()).sum();
        assert_eq!(s.used(), expected);
    }

    #[test]
    fn lazy_and_active_expiry() {
        let mut s = shard();
        put(&mut s, "a", "1", 1000);
        put(&mut s, "b", "2", 2000);
        put(&mut s, "c", "3", 0);
        assert_eq!(s.ttl_keys(), 2);
        assert_eq!(get(&mut s, "a", 999).as_deref(), Some("1"));
        assert_eq!(get(&mut s, "a", 1000), None, "lazy expiry");
        assert_eq!(
            s.active_expire(1500, 100),
            1,
            "stale heap item for 'a' is skipped"
        );
        assert_eq!(s.len(), 2);
        s.active_expire(2000, 100);
        assert_eq!(s.len(), 1);
        assert_eq!(s.ttl_keys(), 0);
        assert_eq!(s.stats.expired, 2);
    }

    #[test]
    fn persist_leaves_stale_deadline_harmless() {
        let mut s = shard();
        put(&mut s, "a", "1", 1000);
        let h = s.hash_of(b"a");
        let i = s.lookup(h, b"a", 0).unwrap();
        s.set_expire(i, h, 0);
        s.active_expire(5000, 100);
        assert_eq!(get(&mut s, "a", 5000).as_deref(), Some("1"));
    }

    #[test]
    fn expiry_heap_is_compacted() {
        let mut s = shard();
        put(&mut s, "a", "1", 10);
        let h = s.hash_of(b"a");
        for t in 11..10_000 {
            let i = s.lookup(h, b"a", 0).unwrap();
            s.set_expire(i, h, t);
        }
        assert!(s.expiries.len() <= 2 * s.ttl_keys() + 1025);
    }

    #[test]
    fn lru_eviction_prefers_old_entries() {
        let mut s = shard();
        for i in 0..100 {
            let k = format!("k{i}");
            let h = s.hash_of(k.as_bytes());
            // Older keys get older clocks.
            s.insert(h, Entry::new(k.as_bytes(), b"v", 0, i));
        }
        let c = clock(1_000_000);
        let mut evicted_ages = 0u64;
        for _ in 0..50 {
            let before: Vec<u32> = s.entries().iter().map(|e| e.meta).collect();
            assert!(s.evict_one(Policy::AllKeysLru, 10, c));
            let after: std::collections::HashSet<u32> =
                s.entries().iter().map(|e| e.meta).collect();
            evicted_ages += before.iter().find(|m| !after.contains(m)).copied().unwrap() as u64;
        }
        // Sampling 10 should evict mostly from the older half (meta < 50); average well below 50.
        assert!(
            evicted_ages / 50 < 40,
            "avg evicted age {}",
            evicted_ages / 50
        );
        assert_eq!(s.stats.evicted, 50);
    }

    #[test]
    fn lfu_counter_grows_logarithmically_and_decays() {
        // Reference values from redis.conf for lfu-log-factor 10: 1K hits -> 18, 100K hits -> 142.
        let c = clock(0);
        let mut counter = LFU_INIT_VAL;
        let mut s = shard();
        for n in 1..=100_000 {
            counter = lfu_incr(counter, s.next_f64());
            if n == 1_000 {
                assert!((12..=24).contains(&counter), "1K hits: counter {counter}");
            }
        }
        assert!(
            (120..=165).contains(&counter),
            "100K hits: counter {counter}"
        );
        let meta = (lfu_minutes(c) << 8) | counter;
        assert_eq!(lfu_decayed(meta, clock(10 * 60_000)), counter - 10);
    }
}
