//! CrabPack: transparent compression of cold values with dictionaries trained per key prefix.
//!
//! Cache values are mostly small (100 B to a few KB) and individually compress poorly, but values that
//! share a key prefix (`user:`, `session:`) tend to share structure: field names, formats, enums. A zstd
//! dictionary trained on samples from one prefix captures that shared structure, so each value can be
//! compressed on its own. Measured on realistic JSON (sessions, products, API responses of ~300 B):
//! plain zstd 1.2-1.5x, zstd with a 16 KB trained dictionary 3.2-3.6x.
//!
//! The flow, driven by the background packing task:
//! 1. Random entries are sampled and grouped by prefix.
//! 2. Once a prefix has enough samples, a dictionary is trained off the I/O threads and kept only if it
//!    compresses held-out samples by at least `MIN_GAIN`.
//! 3. Entries idle for `min_idle` seconds whose prefix has a dictionary are compressed in place.
//!
//! Reads decompress transparently: clients always get the exact bytes they wrote. Any write stores the
//! value plain again, so hot keys stay uncompressed.

use super::entry::{Entry, Packed};
use ahash::RandomState;
use parking_lot::{Mutex, RwLock};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::time::Instant;
use zstd::zstd_safe::{CCtx, CDict, CParameter, DCtx, DDict, DParameter, FrameFormat};

/// Maximum adaptive dictionary size; fixed mode uses this size and level 3.
pub const DICT_SIZE: usize = 16 * 1024;
/// Samples gathered per prefix before training.
pub const SAMPLES_PER_PREFIX: usize = 1000;
/// Training also starts once this many sample bytes are collected.
pub const SAMPLE_BYTES_PER_PREFIX: usize = 256 * 1024;
/// A dictionary must shrink held-out samples by at least this ratio to be kept.
pub const MIN_GAIN: f64 = 1.25;
/// Upper bound on dictionaries (each costs ~16 KB plus prepared zstd tables).
pub const MAX_DICTS: usize = 64;
/// Bound memory even when clients generate many prefixes or very large values.
pub const MAX_SAMPLE_SIZE: usize = 16 * 1024;
pub const DEFAULT_PACK_BYTES: u64 = 512 * 1024;
pub const DEFAULT_PACK_BUDGET_MS: u64 = 2;
/// A prefix whose dictionary was rejected is not sampled again for this long.
pub const REJECT_SECS: u32 = 600;
/// Prefixes are cut at the first ':' within this many bytes.
const MAX_PREFIX: usize = 32;
const LEVEL: i32 = 3;

/// Key group used for dictionaries: everything up to and including the first ':' (within
/// `MAX_PREFIX` bytes), or the empty prefix for keys without one.
pub fn prefix_of(key: &[u8]) -> &[u8] {
    match key.iter().take(MAX_PREFIX).position(|&b| b == b':') {
        Some(i) => &key[..=i],
        None => b"",
    }
}

/// A key prefix and the sample values collected for it, ready for `Codec::train`.
pub type SampleBatch = (Box<[u8]>, Vec<Vec<u8>>);

/// A trained dictionary, prepared for both directions. Accepted dictionaries are never freed (there
/// are at most `MAX_DICTS`), which lets readers use them without locks or reference counting.
pub struct Dict {
    pub id: u32,
    pub prefix: Box<[u8]>,
    /// Compression ratio measured on held-out samples when the dictionary was accepted.
    pub trained_ratio: f64,
    pub prepared_bytes: usize,
    pub raw_bytes: usize,
    pub level: i32,
    cdict: CDict<'static>,
    ddict: DDict<'static>,
}

/// A value could not be decompressed (unknown dictionary or corrupt data). Never expected; reported
/// to the client instead of crashing the server.
#[derive(Debug)]
pub struct Corrupt;

/// Result of trying to train a dictionary for a prefix.
#[derive(Debug, PartialEq)]
pub enum TrainOutcome {
    Accepted { id: u32, ratio: f64 },
    Rejected { ratio: f64 },
    Failed,
}

#[derive(Default)]
struct Sampler {
    seen: HashSet<u64>,
    samples: Vec<Vec<u8>>,
    bytes: usize,
    /// Server-clock second until which the prefix is not sampled (after a rejected dictionary).
    rejected_until: u32,
    training: bool,
}

#[derive(Default)]
pub struct CodecStats {
    pub dicts_rejected: AtomicU64,
    pub values_packed: AtomicU64,
    pub training_jobs: AtomicU64,
    pub training_bytes: AtomicU64,
    pub training_micros: AtomicU64,
    pub pack_micros: AtomicU64,
    pub pack_input_bytes: AtomicU64,
    pub candidates_discarded: AtomicU64,
    pub oversized_skips: AtomicU64,
    pub short_ttl_skips: AtomicU64,
    pub hot_promotions: AtomicU64,
    pub maintenance_backoffs: AtomicU64,
}

pub struct Codec {
    /// Accepted dictionaries by id; written once, read without locks.
    slots: Box<[OnceLock<&'static Dict>]>,
    count: AtomicUsize,
    by_prefix: RwLock<HashMap<Box<[u8]>, &'static Dict>>,
    samplers: Mutex<HashMap<Box<[u8]>, Sampler>>,
    enabled: AtomicBool,
    min_idle_secs: AtomicU64,
    min_size: AtomicUsize,
    pack_max_bytes: AtomicU64,
    pack_budget_ms: AtomicU64,
    sample_hasher: RandomState,
    adaptive_dicts: AtomicBool,
    adaptive_maintenance: AtomicBool,
    min_ttl_secs: AtomicU64,
    hot_max_bytes: AtomicU64,
    pressure_until_ms: AtomicU64,
    pub stats: CodecStats,
}

/// Frame settings: the entry header already records the dictionary and the original length, so the
/// frame omits its magic number, dictionary id and content size (about 7 bytes per value).
fn configure_compressor(c: &mut CCtx<'_>) {
    for param in [
        CParameter::Format(FrameFormat::Magicless),
        CParameter::DictIdFlag(false),
        CParameter::ContentSizeFlag(false),
        CParameter::ChecksumFlag(false),
    ] {
        c.set_parameter(param).expect("valid zstd parameter");
    }
}

fn compressor() -> CCtx<'static> {
    let mut c = CCtx::create();
    configure_compressor(&mut c);
    c
}

fn decompressor() -> DCtx<'static> {
    let mut d = DCtx::create();
    d.set_parameter(DParameter::Format(FrameFormat::Magicless))
        .expect("valid zstd parameter");
    d
}

thread_local! {
    static CCTX: RefCell<CCtx<'static>> = RefCell::new(compressor());
    static DCTX: RefCell<DCtx<'static>> = RefCell::new(decompressor());
    static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Compresses with an already configured context. Returns `None` unless it saves at least 1/8.
fn compress_with<'a>(cctx: &mut CCtx<'a>, cdict: &'a CDict<'_>, value: &[u8]) -> Option<Vec<u8>> {
    cctx.ref_cdict(cdict).ok()?;
    let mut out = Vec::with_capacity(zstd::zstd_safe::compress_bound(value.len()));
    let n = cctx.compress2(&mut &mut out, value).ok()?;
    worthwhile(value.len(), n).then_some(out)
}

/// Savings must cover the packed header as well as the minimum gain.
fn worthwhile(original: usize, compressed: usize) -> bool {
    let packed = compressed.saturating_add(super::entry::PACKED_BYTES);
    packed.saturating_add(original / 8) < original
}

struct Training<'a> {
    stats: &'a CodecStats,
    bytes: u64,
    start: Instant,
}
impl Drop for Training<'_> {
    fn drop(&mut self) {
        self.stats.training_jobs.fetch_sub(1, Relaxed);
        self.stats.training_bytes.fetch_sub(self.bytes, Relaxed);
        self.stats
            .training_micros
            .fetch_add(self.start.elapsed().as_micros() as u64, Relaxed);
    }
}

impl Codec {
    pub fn new(enabled: bool, min_idle_secs: u64, min_size: usize) -> Self {
        Self {
            slots: (0..MAX_DICTS).map(|_| OnceLock::new()).collect(),
            count: AtomicUsize::new(0),
            by_prefix: RwLock::new(HashMap::new()),
            samplers: Mutex::new(HashMap::new()),
            enabled: AtomicBool::new(enabled),
            min_idle_secs: AtomicU64::new(min_idle_secs),
            min_size: AtomicUsize::new(min_size),
            pack_max_bytes: AtomicU64::new(DEFAULT_PACK_BYTES),
            pack_budget_ms: AtomicU64::new(DEFAULT_PACK_BUDGET_MS),
            sample_hasher: RandomState::new(),
            adaptive_dicts: AtomicBool::new(true),
            adaptive_maintenance: AtomicBool::new(true),
            min_ttl_secs: AtomicU64::new(2),
            hot_max_bytes: AtomicU64::new(0),
            pressure_until_ms: AtomicU64::new(0),
            stats: CodecStats::default(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Relaxed)
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Relaxed);
    }

    pub fn min_idle_secs(&self) -> u64 {
        self.min_idle_secs.load(Relaxed)
    }

    pub fn set_min_idle_secs(&self, v: u64) {
        self.min_idle_secs.store(v, Relaxed);
    }

    pub fn min_size(&self) -> usize {
        self.min_size.load(Relaxed)
    }

    pub fn set_min_size(&self, v: usize) {
        self.min_size.store(v, Relaxed);
    }

    pub fn pack_max_bytes(&self) -> u64 {
        self.pack_max_bytes.load(Relaxed)
    }
    pub fn set_pack_max_bytes(&self, bytes: u64) {
        self.pack_max_bytes.store(bytes.max(1), Relaxed);
    }
    pub fn pack_budget_ms(&self) -> u64 {
        self.pack_budget_ms.load(Relaxed)
    }
    pub fn set_pack_budget_ms(&self, ms: u64) {
        self.pack_budget_ms.store(ms.clamp(1, 1000), Relaxed);
    }
    pub fn adaptive_dicts(&self) -> bool {
        self.adaptive_dicts.load(Relaxed)
    }
    pub fn set_adaptive_dicts(&self, on: bool) {
        self.adaptive_dicts.store(on, Relaxed);
    }
    pub fn adaptive_maintenance(&self) -> bool {
        self.adaptive_maintenance.load(Relaxed)
    }
    pub fn set_adaptive_maintenance(&self, on: bool) {
        self.adaptive_maintenance.store(on, Relaxed);
    }
    pub fn min_ttl_secs(&self) -> u64 {
        self.min_ttl_secs.load(Relaxed)
    }
    pub fn set_min_ttl_secs(&self, secs: u64) {
        self.min_ttl_secs.store(secs, Relaxed);
    }
    pub fn hot_max_bytes(&self) -> u64 {
        self.hot_max_bytes.load(Relaxed)
    }
    pub fn set_hot_max_bytes(&self, bytes: u64) {
        self.hot_max_bytes.store(bytes, Relaxed);
    }
    pub fn ttl_eligible(&self, e: &Entry, now_ms: u64) -> bool {
        let at = e.expire_at();
        at == 0 || at.saturating_sub(now_ms) >= self.min_ttl_secs().saturating_mul(1000)
    }
    /// Sampled command execution time, excluding socket reads/writes; not CPU time.
    pub fn observe_request(&self, elapsed_us: u64, now_ms: u64) {
        if elapsed_us >= 1000 {
            self.pressure_until_ms
                .fetch_max(now_ms.saturating_add(1000), Relaxed);
        }
    }
    pub fn under_pressure(&self, now_ms: u64) -> bool {
        self.adaptive_maintenance() && now_ms < self.pressure_until_ms.load(Relaxed)
    }

    pub fn has_dicts(&self) -> bool {
        self.count.load(Relaxed) != 0
    }
    /// Excludes allocator slack; training buffers are reported separately.
    pub fn sampler_memory(&self) -> (usize, usize) {
        let samplers = self.samplers.lock();
        (
            samplers.len(),
            samplers
                .values()
                .map(|s| s.bytes + s.seen.capacity() * 8)
                .sum(),
        )
    }

    pub fn dicts(&self) -> Vec<&'static Dict> {
        self.slots.iter().filter_map(|s| s.get().copied()).collect()
    }

    pub fn dict(&self, id: u32) -> Option<&'static Dict> {
        self.slots.get(id as usize)?.get().copied()
    }

    pub fn dict_for_key(&self, key: &[u8]) -> Option<&'static Dict> {
        self.by_prefix.read().get(prefix_of(key)).copied()
    }

    /// Calls `f` with the value of `e` as clients see it, decompressing it if needed.
    pub fn with_value<R>(&self, e: &Entry, f: impl FnOnce(&[u8]) -> R) -> Result<R, Corrupt> {
        let Some(p) = e.packed() else {
            return Ok(f(e.stored()));
        };
        self.with_packed_value(p, e.stored(), f)
    }

    /// An owned copy of the compressed bytes can be decoded after releasing a shard.
    pub fn with_packed_value<R>(
        &self,
        p: Packed,
        stored: &[u8],
        f: impl FnOnce(&[u8]) -> R,
    ) -> Result<R, Corrupt> {
        let dict = self.dict(p.dict).ok_or(Corrupt)?;
        SCRATCH.with(|scratch| {
            let mut buf = scratch.borrow_mut();
            buf.clear();
            buf.reserve(p.original_len as usize);
            let n = DCTX
                .with(|d| {
                    let mut d = d.borrow_mut();
                    d.ref_ddict(&dict.ddict)?;
                    d.decompress(&mut &mut *buf, stored)
                })
                .map_err(|_| Corrupt)?;
            if n != p.original_len as usize {
                return Err(Corrupt);
            }
            Ok(f(&buf))
        })
    }

    /// Append exactly one value without a scratch-to-output copy. On any error, restore the
    /// previous initialized output; Cursor delegates initialized-length tracking to zstd-safe.
    pub fn append_packed(
        &self,
        p: Packed,
        stored: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<(), Corrupt> {
        let dict = self.dict(p.dict).ok_or(Corrupt)?;
        let start = out.len();
        out.reserve(p.original_len as usize);
        let result = DCTX.with(|d| {
            let mut d = d.borrow_mut();
            d.ref_ddict(&dict.ddict)?;
            let mut destination = std::io::Cursor::new(&mut *out);
            destination.set_position(start as u64);
            d.decompress(&mut destination, stored)
        });
        if result.ok() != Some(p.original_len as usize) {
            out.truncate(start);
            return Err(Corrupt);
        }
        Ok(())
    }

    /// Compresses `value` with `dict`. Returns `None` when it would not save at least 1/8 of the size.
    pub fn compress(&self, dict: &'static Dict, value: &[u8]) -> Option<Vec<u8>> {
        CCTX.with(|c| compress_with(&mut c.borrow_mut(), &dict.cdict, value))
    }

    /// Builds the compressed replacement for a plain entry, if worthwhile.
    pub fn pack(&self, dict: &'static Dict, e: &Entry) -> Option<Entry> {
        let value = e.plain()?;
        let compressed = self.compress(dict, value)?;
        let packed = Packed {
            dict: dict.id,
            original_len: value.len() as u32,
        };
        Some(Entry::new_packed(
            e.key(),
            &compressed,
            e.expire_at(),
            e.meta,
            packed,
        ))
    }

    /// Whether values with this key should still be sampled.
    pub fn wants_samples(&self, key: &[u8], now_secs: u32) -> bool {
        if self.count.load(Relaxed) >= MAX_DICTS || self.dict_for_key(key).is_some() {
            return false;
        }
        let samplers = self.samplers.lock();
        match samplers.get(prefix_of(key)) {
            Some(s) => !s.training && s.rejected_until <= now_secs,
            None => {
                samplers.len() + self.count.load(Relaxed) < MAX_DICTS
                    || samplers.values().any(|s| {
                        !s.training && s.samples.is_empty() && s.rejected_until <= now_secs
                    })
            }
        }
    }

    pub(crate) fn sample_fingerprint(&self, key: &[u8]) -> u64 {
        self.sample_hasher.hash_one(key)
    }

    pub fn offer_sample(&self, key: &[u8], value: &[u8], now_secs: u32) -> Option<SampleBatch> {
        self.offer_sample_owned(
            prefix_of(key),
            self.sample_fingerprint(key),
            value[..value.len().min(MAX_SAMPLE_SIZE)].to_vec(),
            now_secs,
        )
    }

    /// Transfers the sample allocation rather than copying its bytes a second time.
    pub fn offer_sample_owned(
        &self,
        prefix: &[u8],
        key_hash: u64,
        mut value: Vec<u8>,
        now_secs: u32,
    ) -> Option<SampleBatch> {
        if self.count.load(Relaxed) >= MAX_DICTS || self.dict_for_key(prefix).is_some() {
            return None;
        }
        let mut samplers = self.samplers.lock();
        if !samplers.contains_key(prefix) && samplers.len() + self.count.load(Relaxed) >= MAX_DICTS
        {
            let expired = samplers
                .iter()
                .find(|(_, s)| !s.training && s.samples.is_empty() && s.rejected_until <= now_secs)
                .map(|(p, _)| p.clone());
            let expired = expired?;
            samplers.remove(&expired);
        }
        let s = samplers.entry(prefix.into()).or_default();
        if s.training || s.rejected_until > now_secs || !s.seen.insert(key_hash) {
            return None;
        }
        value.truncate(MAX_SAMPLE_SIZE);
        // Copy callers clip before allocation; owned callers are bounded as well.
        value.shrink_to_fit();
        s.bytes += value.len();
        s.samples.push(value);
        if s.samples.len() >= SAMPLES_PER_PREFIX || s.bytes >= SAMPLE_BYTES_PER_PREFIX {
            s.training = true;
            s.bytes = 0;
            s.seen = HashSet::new();
            return Some((prefix.into(), std::mem::take(&mut s.samples)));
        }
        None
    }

    /// Trains a dictionary from `samples` (CPU heavy: run it on a blocking thread). The last fifth of
    /// the samples is held out to measure the gain; the dictionary is registered only if it reaches
    /// `MIN_GAIN`.
    pub fn train(&self, prefix: Box<[u8]>, samples: Vec<Vec<u8>>, now_secs: u32) -> TrainOutcome {
        let bytes = samples.iter().map(|v| v.len() as u64).sum();
        self.stats.training_jobs.fetch_add(1, Relaxed);
        self.stats.training_bytes.fetch_add(bytes, Relaxed);
        let _training = Training {
            stats: &self.stats,
            bytes,
            start: Instant::now(),
        };
        let outcome = self.try_train(&prefix, &samples);
        let mut samplers = self.samplers.lock();
        if matches!(outcome, TrainOutcome::Accepted { .. }) {
            samplers.remove(&prefix);
            return outcome;
        }
        let s = samplers.entry(prefix).or_default();
        s.training = false;
        if !matches!(outcome, TrainOutcome::Accepted { .. }) {
            s.rejected_until = now_secs.saturating_add(REJECT_SECS);
            self.stats.dicts_rejected.fetch_add(1, Relaxed);
        }
        outcome
    }

    fn try_train(&self, prefix: &[u8], samples: &[Vec<u8>]) -> TrainOutcome {
        if samples.len() < 10 {
            return TrainOutcome::Failed;
        }
        let split = samples.len() * 4 / 5;
        let (train, test) = samples.split_at(split);
        let adaptive = self.adaptive_dicts();
        let mut best = None;
        let mut best_cost = usize::MAX;
        let mut best_ratio: f64 = 1.0;
        let original: usize = test.iter().map(Vec::len).sum();
        // Evaluate prepared tables, not just raw dictionary size. Use only the observed population
        // to amortize fixed cost; future key counts are unknown. Fixed mode retains old admission.
        let sizes: &[usize] = if adaptive {
            &[4096, 8192, DICT_SIZE]
        } else {
            &[DICT_SIZE]
        };
        let levels: &[i32] = if adaptive { &[1, LEVEL] } else { &[LEVEL] };
        for &size in sizes {
            let Ok(raw) = zstd::dict::from_samples(train, size) else {
                continue;
            };
            for &level in levels {
                let cdict = CDict::create(&raw, level);
                let ddict = DDict::create(&raw);
                let prepared =
                    cdict.sizeof() + ddict.sizeof() + prefix.len() + std::mem::size_of::<Dict>();
                let (stored, physical) = {
                    let mut cctx = compressor();
                    let mut stored = 0usize;
                    let mut physical = 0usize;
                    for value in test {
                        if let Some(packed) = compress_with(&mut cctx, &cdict, value) {
                            stored += packed.len();
                            physical += packed.len() + super::entry::PACKED_BYTES;
                        } else {
                            stored += value.len();
                            physical += value.len();
                        }
                    }
                    (stored, physical)
                };
                let ratio = original as f64 / stored.max(1) as f64;
                best_ratio = best_ratio.max(ratio);
                let population_cost = physical.saturating_mul(samples.len()) / test.len().max(1);
                let cost = prepared.saturating_add(population_cost);
                let population_plain = original.saturating_mul(samples.len()) / test.len().max(1);
                if ratio >= MIN_GAIN && (!adaptive || cost < population_plain) && cost < best_cost {
                    best_cost = cost;
                    best = Some((raw.len(), level, ratio, prepared, cdict, ddict));
                }
            }
        }
        let Some((raw_bytes, level, ratio, prepared_bytes, cdict, ddict)) = best else {
            return TrainOutcome::Rejected { ratio: best_ratio };
        };
        let mut by_prefix = self.by_prefix.write();
        let id = self.count.load(Relaxed);
        if id >= MAX_DICTS {
            return TrainOutcome::Failed;
        }
        let dict: &'static Dict = Box::leak(Box::new(Dict {
            id: id as u32,
            prefix: prefix.into(),
            trained_ratio: ratio,
            prepared_bytes,
            raw_bytes,
            level,
            cdict,
            ddict,
        }));
        // Registration happens under the by_prefix write lock, so ids are assigned one at a time.
        let _ = self.slots[id].set(dict);
        self.count.store(id + 1, Relaxed);
        by_prefix.insert(prefix.into(), dict);
        TrainOutcome::Accepted {
            id: id as u32,
            ratio,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Deterministic JSON-like values with shared structure, like sessions in a real cache.
    pub(crate) fn sample_values(n: usize, seed: u64) -> Vec<Vec<u8>> {
        let mut s = seed | 1;
        let mut next = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let names = [
            "Ana", "Bruno", "Carla", "Diego", "Elisa", "Felipe", "Gabriela", "Hugo",
        ];
        let themes = ["dark", "light", "system"];
        (0..n)
            .map(|_| {
                let a = next();
                let b = next();
                format!(
                    r#"{{"user_id":{},"name":"{} {}","email":"user{}@example.com","roles":["user"],"locale":"pt-BR","theme":"{}","last_seen":"2026-{:02}-{:02}T{:02}:{:02}:00Z","csrf":"{:016x}"}}"#,
                    a % 10_000_000,
                    names[(a % 8) as usize],
                    names[(b % 8) as usize],
                    b % 100_000,
                    themes[(a % 3) as usize],
                    1 + a % 12,
                    1 + b % 28,
                    a % 24,
                    b % 60,
                    a ^ b
                )
                .into_bytes()
            })
            .collect()
    }

    #[test]
    fn prefixes() {
        assert_eq!(prefix_of(b"user:42"), b"user:");
        assert_eq!(prefix_of(b"a:b:c"), b"a:");
        assert_eq!(prefix_of(b"plain"), b"");
        assert_eq!(prefix_of(b""), b"");
        let long = [b'x'; 40];
        let mut key = long.to_vec();
        key.push(b':');
        assert_eq!(
            prefix_of(&key),
            b"",
            "a ':' beyond MAX_PREFIX does not count"
        );
    }

    #[test]
    fn trains_compresses_and_round_trips() {
        let codec = Codec::new(true, 0, 64);
        let samples = sample_values(SAMPLES_PER_PREFIX, 7);
        let outcome = codec.train(b"session:"[..].into(), samples, 0);
        let TrainOutcome::Accepted { id, ratio } = outcome else {
            panic!("dictionary rejected: {outcome:?}");
        };
        assert!(ratio >= MIN_GAIN, "ratio {ratio}");
        let dict = codec
            .dict_for_key(b"session:abc")
            .expect("registered for its prefix");
        assert_eq!(dict.id, id);
        assert!(codec.dict_for_key(b"user:1").is_none());

        // Fresh values (different seed) round-trip byte for byte through a packed entry.
        let mut saved = 0;
        for v in sample_values(200, 99) {
            let e = Entry::new(b"session:x", &v, 1234, 5);
            let packed = codec.pack(dict, &e).expect("compressible");
            assert_eq!(
                packed.packed().map(|p| p.original_len as usize),
                Some(v.len())
            );
            assert_eq!(
                (packed.expire_at(), packed.meta, packed.key()),
                (1234, 5, &b"session:x"[..])
            );
            let back = codec.with_value(&packed, |b| b.to_vec()).unwrap();
            assert_eq!(back, v);
            saved += e.mem_usage() - packed.mem_usage();
        }
        assert!(saved > 0);
    }

    #[test]
    fn rejects_dictionaries_that_do_not_help() {
        let codec = Codec::new(true, 0, 64);
        let mut s = 0x1234_5678_9abc_def0_u64;
        let random: Vec<Vec<u8>> = (0..500)
            .map(|_| {
                (0..200)
                    .map(|_| {
                        s ^= s << 13;
                        s ^= s >> 7;
                        s ^= s << 17;
                        s as u8
                    })
                    .collect()
            })
            .collect();
        let outcome = codec.train(b"blob:"[..].into(), random, 100);
        assert!(
            !matches!(outcome, TrainOutcome::Accepted { .. }),
            "{outcome:?}"
        );
        assert!(codec.dict_for_key(b"blob:1").is_none());
        // The prefix is not sampled again until the rejection expires.
        assert!(!codec.wants_samples(b"blob:1", 100 + REJECT_SECS - 1));
        assert!(codec.wants_samples(b"blob:1", 100 + REJECT_SECS));
    }

    #[test]
    fn sampler_hands_over_a_batch_once() {
        let codec = Codec::new(true, 0, 64);
        let v = b"{\"a\":1}";
        let mut batches = 0;
        for i in 0..SAMPLES_PER_PREFIX + 10 {
            if codec
                .offer_sample(format!("k:{i}").as_bytes(), v, 0)
                .is_some()
            {
                batches += 1;
            }
        }
        assert_eq!(batches, 1, "a prefix being trained gets no more samples");
        assert!(!codec.wants_samples(b"k:2", 0));
    }

    #[test]
    fn unknown_dictionary_is_reported_not_panicking() {
        let codec = Codec::new(true, 0, 64);
        let e = Entry::new_packed(
            b"k",
            b"garbage",
            0,
            0,
            Packed {
                dict: 9,
                original_len: 10,
            },
        );
        assert!(codec.with_value(&e, |_| ()).is_err());
    }
    #[test]
    fn tiny_value_admission_counts_packed_header() {
        assert!(
            !worthwhile(40, 35),
            "five saved bytes cannot pay for an eight byte header"
        );
        assert!(!worthwhile(40, 27));
        assert!(worthwhile(40, 26));
        assert!(!worthwhile(128, 104));
        assert!(worthwhile(128, 103));
        assert!(!worthwhile(128, usize::MAX));
    }

    #[test]
    fn repeated_keys_cannot_leak_into_training_and_held_out_samples() {
        let codec = Codec::new(true, 0, 64);
        for _ in 0..SAMPLES_PER_PREFIX * 2 {
            assert!(codec.offer_sample(b"same:1", &[b'x'; 300], 0).is_none());
        }
        let samplers = codec.samplers.lock();
        assert_eq!(samplers[b"same:".as_slice()].samples.len(), 1);
    }

    #[test]
    fn sampler_bounds_prefixes_and_large_values() {
        let codec = Codec::new(true, 0, 64);
        let value = vec![b'x'; 1024 * 1024];
        for i in 0..MAX_DICTS * 2 {
            assert!(
                codec
                    .offer_sample(format!("p{i}:k").as_bytes(), &value, 0)
                    .is_none()
            );
        }
        let samplers = codec.samplers.lock();
        assert_eq!(samplers.len(), MAX_DICTS);
        assert!(
            samplers
                .values()
                .all(|s| s.bytes == MAX_SAMPLE_SIZE && s.samples[0].capacity() <= MAX_SAMPLE_SIZE)
        );
    }

    #[test]
    fn expired_rejection_can_release_sampler_slot() {
        let codec = Codec::new(true, 0, 64);
        {
            let mut samplers = codec.samplers.lock();
            for i in 0..MAX_DICTS {
                samplers.insert(
                    format!("p{i}:").into_bytes().into_boxed_slice(),
                    Sampler {
                        rejected_until: 100,
                        ..Sampler::default()
                    },
                );
            }
        }
        assert!(!codec.wants_samples(b"new:k", 99));
        assert!(codec.wants_samples(b"new:k", 100));
        codec.offer_sample(b"new:k", b"sample", 100);
        assert_eq!(codec.samplers.lock().len(), MAX_DICTS);
        assert!(codec.samplers.lock().contains_key(b"new:".as_slice()));
    }

    #[test]
    fn owned_packed_bytes_survive_overwrite_and_disabling_compression() {
        let codec = Codec::new(true, 0, 64);
        assert!(matches!(
            codec.train(b"session:".as_slice().into(), sample_values(1000, 42), 0),
            TrainOutcome::Accepted { .. }
        ));
        let expected = sample_values(1, 73).remove(0);
        let dict = codec.dict_for_key(b"session:k").unwrap();
        let mut e = codec
            .pack(dict, &Entry::new(b"session:k", &expected, 0, 0))
            .unwrap();
        let p = e.packed().unwrap();
        let owned = e.stored().to_vec();
        e.set_value(b"replacement");
        drop(e);
        codec.set_enabled(false);
        assert_eq!(
            codec.with_packed_value(p, &owned, |v| v.to_vec()).unwrap(),
            expected
        );
        assert_eq!(codec.stats.training_jobs.load(Relaxed), 0);
        assert_eq!(codec.stats.training_bytes.load(Relaxed), 0);
        assert!(dict.prepared_bytes > DICT_SIZE);
    }
}

#[cfg(test)]
mod adaptive_tests {
    use super::tests::sample_values;
    use super::*;

    #[test]
    fn adaptive_dictionary_optimizes_observed_total_not_only_ratio() {
        let values = sample_values(1000, 123);
        let fixed = Codec::new(true, 0, 64);
        fixed.set_adaptive_dicts(false);
        let adaptive = Codec::new(true, 0, 64);
        for codec in [&fixed, &adaptive] {
            assert!(matches!(
                codec.train(b"session:".as_slice().into(), values.clone(), 0),
                TrainOutcome::Accepted { .. }
            ));
        }
        let cost = |codec: &Codec| {
            let d = codec.dict(0).unwrap();
            let payload: usize = values[800..]
                .iter()
                .map(|v| {
                    codec
                        .compress(d, v)
                        .map_or(v.len(), |c| c.len() + super::super::entry::PACKED_BYTES)
                })
                .sum();
            d.prepared_bytes + payload * 5
        };
        assert!(cost(&adaptive) <= cost(&fixed));
        assert!(adaptive.dict(0).unwrap().prepared_bytes < fixed.dict(0).unwrap().prepared_bytes);
        assert!(adaptive.dict(0).unwrap().raw_bytes <= DICT_SIZE);
    }

    #[test]
    fn prepared_memory_cannot_be_amortized_by_a_tiny_population() {
        let codec = Codec::new(true, 0, 40);
        assert!(!matches!(
            codec.train(b"small:".as_slice().into(), vec![vec![b'x'; 100]; 20], 0),
            TrainOutcome::Accepted { .. }
        ));
        assert_eq!(codec.dicts().len(), 0);
    }

    #[test]
    fn direct_decode_appends_binary_and_rolls_back_on_length_or_frame_errors() {
        let codec = Codec::new(true, 0, 64);
        codec.train(b"session:".as_slice().into(), sample_values(1000, 123), 0);
        let value = [
            sample_values(1, 32).remove(0).repeat(300),
            b"\x00\xff\xfe\r\n".to_vec(),
        ]
        .concat();
        let dict = codec.dict(0).unwrap();
        let e = codec
            .pack(dict, &Entry::new(b"session:test", &value, 0, 0))
            .unwrap();
        let p = e.packed().unwrap();
        let prefix = b"+OK\r\n*3\r\n";
        let mut out = prefix.to_vec();
        codec.append_packed(p, e.stored(), &mut out).unwrap();
        assert_eq!(&out[prefix.len()..], value);
        for (packed, frame) in [
            (
                Packed {
                    original_len: p.original_len + 1,
                    ..p
                },
                e.stored(),
            ),
            (
                Packed {
                    original_len: p.original_len - 1,
                    ..p
                },
                e.stored(),
            ),
            (p, &e.stored()[..e.stored().len() / 2]),
            (Packed { dict: 63, ..p }, e.stored()),
        ] {
            let mut out = prefix.to_vec();
            assert!(codec.append_packed(packed, frame, &mut out).is_err());
            assert_eq!(out, prefix);
        }
    }

    #[test]
    fn ttl_and_pressure_controls_have_explicit_boundaries() {
        let codec = Codec::new(true, 0, 64);
        assert!(codec.ttl_eligible(&Entry::new(b"k", b"v", 0, 0), 100));
        assert!(codec.ttl_eligible(&Entry::new(b"k", b"v", 2100, 0), 100));
        assert!(!codec.ttl_eligible(&Entry::new(b"k", b"v", 2099, 0), 100));
        codec.set_min_ttl_secs(0);
        assert!(codec.ttl_eligible(&Entry::new(b"k", b"v", 101, 0), 100));
        codec.observe_request(999, 100);
        assert!(!codec.under_pressure(100));
        codec.observe_request(1000, 100);
        assert!(codec.under_pressure(1099));
        assert!(!codec.under_pressure(1100));
        codec.set_adaptive_maintenance(false);
        assert!(!codec.under_pressure(100));
    }
}
