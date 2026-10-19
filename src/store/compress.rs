//! CrabPack: adaptive compression of cold values with dynamic per-prefix dictionaries and cost-benefit control.
//!
//! Cache values are mostly small (100 B to a few KB) and individually compress poorly, but values that
//! share a key prefix (`user:`, `session:`) tend to share structure: field names, formats, enums. A zstd
//! dictionary trained on samples from one prefix captures that shared structure, so each value can be
//! compressed on its own. Measured on realistic JSON (sessions, products, API responses of ~300 B):
//! plain zstd 1.2-1.5x, zstd with a 16 KB trained dictionary 3.2-3.6x.
//!
//! CrabPack combines three adaptive mechanisms:
//! 1. **Adaptive algorithm selection (`Zstd`, `LZ4`, or `Plain`)**: chooses between prefix-dictionary Zstd,
//!    dictionary-free LZ4 block compression, or keeping the entry uncompressed based on value size, LFU
//!    access frequency, prefix dictionary availability, and achieved byte savings.
//! 2. **Dynamic versioned dictionaries**: monitors live compression ratios per prefix to detect schema or
//!    distribution drift, retrains a new dictionary version in the background, keeps retired versions alive
//!    for as long as existing packed entries reference them, and reclaims retired dictionary memory once
//!    the last dependent entry is updated, migrated, or removed.
//! 3. **Cost-benefit controller**: accounts for real net memory savings (including the 8-byte packed header,
//!    access clocks, sampler buffers, and prepared zstd tables) alongside rolling P95/P99 command latency
//!    and decompression CPU cost.

use super::entry::{Entry, LZ4_DICT_ID, PACKED_BYTES, Packed};
use ahash::RandomState;
use parking_lot::{Mutex, RwLock};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize,
    Ordering::{Acquire, Relaxed, SeqCst},
};
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
/// Upper bound on active prefix dictionaries (each costs ~16 KB plus prepared zstd tables).
pub const MAX_DICTS: usize = 64;
/// Bound memory even when clients generate many prefixes or very large values.
pub const MAX_SAMPLE_SIZE: usize = 16 * 1024;
pub const DEFAULT_PACK_BYTES: u64 = 512 * 1024;
pub const DEFAULT_PACK_BUDGET_MS: u64 = 2;
pub const DEFAULT_LATENCY_TARGET_US: u64 = 1000;
/// A prefix whose dictionary was rejected is not sampled again for this long.
pub const REJECT_SECS: u32 = 600;
/// Number of evaluations per window before checking whether a dictionary has drifted.
pub const DRIFT_WINDOW: u64 = 32;
/// Ratio drop relative to `trained_ratio` that flags schema/data drift on a prefix.
pub const DRIFT_RATIO_FACTOR: f64 = 0.75;
/// LFU counter above which adaptive mode prefers LZ4 over Zstd to reduce read CPU.
pub const WARM_LFU_THRESHOLD: u8 = 12;
/// LFU counter above which adaptive mode under latency pressure skips packing altogether.
pub const HOT_SKIP_LFU_THRESHOLD: u8 = 24;
/// Value size above which adaptive mode considers LZ4 even when a prefix dictionary exists.
pub const LARGE_LZ4_THRESHOLD: usize = 4096;
/// Prefixes are cut at the first ':' within this many bytes.
const MAX_PREFIX: usize = 32;
const LEVEL: i32 = 3;
const LATENCY_WINDOW: usize = 64;

/// Key group used for dictionaries: everything up to and including the first ':' (within
/// `MAX_PREFIX` bytes), or the empty prefix for keys without one.
pub fn prefix_of(key: &[u8]) -> &[u8] {
    match key.iter().take(MAX_PREFIX).position(|&b| b == b':') {
        Some(i) => &key[..=i],
        None => b"",
    }
}

/// Algorithm policy for value compression.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum CompressionAlgo {
    /// Dynamically select Zstd dictionary, LZ4 block, or plain storage per value.
    Auto = 0,
    /// Only compress with trained Zstd prefix dictionaries.
    Zstd = 1,
    /// Only compress with dictionary-free LZ4 block compression.
    Lz4 = 2,
}

impl CompressionAlgo {
    pub fn parse(s: &[u8]) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_slice() {
            b"auto" | b"adaptive" => Self::Auto,
            b"zstd" => Self::Zstd,
            b"lz4" => Self::Lz4,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Zstd => "zstd",
            Self::Lz4 => "lz4",
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Zstd,
            2 => Self::Lz4,
            _ => Self::Auto,
        }
    }
}

/// A key prefix and the sample values collected for it, ready for `Codec::train`.
pub type SampleBatch = (Box<[u8]>, Vec<Vec<u8>>);

/// A trained dictionary version, prepared for both directions.
///
/// When a prefix's data structure changes and a newer dictionary version is trained, the old
/// dictionary is marked `retired` and kept in the slot registry for as long as `active_keys > 0`.
/// Once the last dependent entry is updated, migrated, or removed, the retired dictionary is
/// dropped and its prepared zstd tables are freed.
pub struct Dict {
    pub id: u32,
    pub version: u32,
    pub prefix: Box<[u8]>,
    /// Compression ratio measured on held-out samples when the dictionary was accepted.
    pub trained_ratio: f64,
    pub prepared_bytes: usize,
    pub raw_bytes: usize,
    pub level: i32,
    pub active_keys: AtomicUsize,
    pub live_original_bytes: AtomicU64,
    pub live_stored_bytes: AtomicU64,
    pub retired: AtomicBool,
    pub drifted: AtomicBool,
    recent_evals: AtomicU64,
    recent_original_bytes: AtomicU64,
    recent_stored_bytes: AtomicU64,
    recent_rejects: AtomicU64,
    cdict: CDict<'static>,
    ddict: DDict<'static>,
}

impl AsRef<Dict> for Dict {
    fn as_ref(&self) -> &Dict {
        self
    }
}

impl Dict {
    pub fn is_retired(&self) -> bool {
        self.retired.load(Acquire)
    }

    pub fn is_drifted(&self) -> bool {
        self.drifted.load(Relaxed)
    }

    pub fn active_key_count(&self) -> usize {
        self.active_keys.load(Acquire)
    }

    /// Net memory saved by entries currently compressed with this dictionary version,
    /// subtracting the 8-byte per-entry header and this dictionary's prepared tables.
    pub fn net_saved_bytes(&self) -> i64 {
        let orig = self.live_original_bytes.load(Relaxed) as i64;
        let stored = self.live_stored_bytes.load(Relaxed) as i64;
        let headers = (self.active_keys.load(Relaxed) * PACKED_BYTES) as i64;
        orig - stored - headers - self.prepared_bytes as i64
    }

    /// Live compression ratio across entries currently stored with this dictionary.
    pub fn live_ratio(&self) -> f64 {
        let orig = self.live_original_bytes.load(Relaxed);
        let stored = self.live_stored_bytes.load(Relaxed);
        if stored == 0 {
            self.trained_ratio
        } else {
            orig as f64 / stored as f64
        }
    }

    /// Records the result of compressing a plain value of `original_len` with this dictionary
    /// and returns `true` if this observation triggered a drift transition.
    fn observe_eval(&self, original_len: usize, compressed_len: Option<usize>) -> bool {
        if self.is_retired() || self.is_drifted() {
            return false;
        }
        self.recent_original_bytes
            .fetch_add(original_len as u64, Relaxed);
        match compressed_len {
            Some(c) => {
                self.recent_stored_bytes
                    .fetch_add((c + PACKED_BYTES) as u64, Relaxed);
            }
            None => {
                self.recent_stored_bytes
                    .fetch_add(original_len as u64, Relaxed);
                self.recent_rejects.fetch_add(1, Relaxed);
            }
        }
        let evals = self.recent_evals.fetch_add(1, Relaxed) + 1;
        if evals < DRIFT_WINDOW {
            return false;
        }
        let orig = self.recent_original_bytes.swap(0, Relaxed);
        let stored = self.recent_stored_bytes.swap(0, Relaxed);
        let rejects = self.recent_rejects.swap(0, Relaxed);
        self.recent_evals.store(0, Relaxed);
        if orig == 0 || stored == 0 {
            return false;
        }
        let observed_ratio = orig as f64 / stored as f64;
        let threshold = (self.trained_ratio * DRIFT_RATIO_FACTOR).max(MIN_GAIN);
        if observed_ratio < threshold || rejects.saturating_mul(2) >= evals {
            return !self.drifted.swap(true, Relaxed);
        }
        false
    }
}

/// Shared registry mapping dictionary IDs to live `Arc<Dict>` instances and tracking shard references.
#[derive(Default)]
pub struct DictRegistry {
    slots: RwLock<Vec<Option<Arc<Dict>>>>,
    by_prefix: RwLock<HashMap<Box<[u8]>, Arc<Dict>>>,
    pub reclaimed: AtomicU64,
}

impl DictRegistry {
    pub fn get(&self, id: u32) -> Option<Arc<Dict>> {
        if id == LZ4_DICT_ID {
            return None;
        }
        self.slots.read().get(id as usize)?.clone()
    }

    pub fn inc_ref(&self, id: u32, keys: usize, original_bytes: u64, stored_bytes: u64) {
        if id == LZ4_DICT_ID || keys == 0 {
            return;
        }
        if let Some(dict) = self.get(id) {
            dict.active_keys.fetch_add(keys, Relaxed);
            dict.live_original_bytes.fetch_add(original_bytes, Relaxed);
            dict.live_stored_bytes.fetch_add(stored_bytes, Relaxed);
        }
    }

    pub fn dec_ref(&self, id: u32, keys: usize, original_bytes: u64, stored_bytes: u64) {
        if id == LZ4_DICT_ID || keys == 0 {
            return;
        }
        let should_reclaim = if let Some(dict) = self.get(id) {
            atomic_saturating_sub_u64(&dict.live_original_bytes, original_bytes);
            atomic_saturating_sub_u64(&dict.live_stored_bytes, stored_bytes);
            let prev = atomic_saturating_sub_usize(&dict.active_keys, keys);
            prev <= keys && dict.is_retired()
        } else {
            false
        };
        if should_reclaim {
            self.try_reclaim_slot(id);
        }
    }

    fn try_reclaim_slot(&self, id: u32) -> bool {
        let mut slots = self.slots.write();
        if let Some(slot) = slots.get_mut(id as usize)
            && let Some(dict) = slot.as_ref()
            && dict.is_retired()
            && dict.active_keys.load(SeqCst) == 0
        {
            *slot = None;
            self.reclaimed.fetch_add(1, Relaxed);
            return true;
        }
        false
    }
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
    pub dicts_retrained: AtomicU64,
    pub dict_drifts: AtomicU64,
    pub values_packed: AtomicU64,
    pub zstd_packed: AtomicU64,
    pub lz4_packed: AtomicU64,
    pub training_jobs: AtomicU64,
    pub training_bytes: AtomicU64,
    pub training_micros: AtomicU64,
    pub pack_micros: AtomicU64,
    pub pack_input_bytes: AtomicU64,
    pub decompress_micros: AtomicU64,
    pub decompress_calls: AtomicU64,
    pub candidates_discarded: AtomicU64,
    pub oversized_skips: AtomicU64,
    pub short_ttl_skips: AtomicU64,
    pub cost_benefit_skips: AtomicU64,
    pub hot_promotions: AtomicU64,
    pub maintenance_backoffs: AtomicU64,
}

pub struct Codec {
    registry: Arc<DictRegistry>,
    samplers: Mutex<HashMap<Box<[u8]>, Sampler>>,
    enabled: AtomicBool,
    algorithm: AtomicU8,
    min_idle_secs: AtomicU64,
    min_size: AtomicUsize,
    pack_max_bytes: AtomicU64,
    pack_budget_ms: AtomicU64,
    sample_hasher: RandomState,
    adaptive_dicts: AtomicBool,
    adaptive_maintenance: AtomicBool,
    min_ttl_secs: AtomicU64,
    hot_max_bytes: AtomicU64,
    latency_target_us: AtomicU64,
    pressure_until_ms: AtomicU64,
    latency_ring: [AtomicU64; LATENCY_WINDOW],
    latency_cursor: AtomicU32,
    latency_count: AtomicU64,
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

fn atomic_saturating_sub_u64(atom: &AtomicU64, sub: u64) -> u64 {
    let mut cur = atom.load(Relaxed);
    loop {
        let next = cur.saturating_sub(sub);
        match atom.compare_exchange_weak(cur, next, Relaxed, Relaxed) {
            Ok(_) => return cur,
            Err(actual) => cur = actual,
        }
    }
}

fn atomic_saturating_sub_usize(atom: &AtomicUsize, sub: usize) -> usize {
    let mut cur = atom.load(SeqCst);
    loop {
        let next = cur.saturating_sub(sub);
        match atom.compare_exchange_weak(cur, next, SeqCst, SeqCst) {
            Ok(_) => return cur,
            Err(actual) => cur = actual,
        }
    }
}

fn compressor<'a>() -> CCtx<'a> {
    let mut c = CCtx::create();
    configure_compressor(&mut c);
    c
}

fn decompressor<'a>() -> DCtx<'a> {
    let mut d = DCtx::create();
    d.set_parameter(DParameter::Format(FrameFormat::Magicless))
        .expect("valid zstd parameter");
    d
}

thread_local! {
    static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Compresses with an already configured context. Returns `None` unless it saves at least 1/8.
fn compress_with<'a>(cctx: &mut CCtx<'a>, cdict: &'a CDict<'_>, value: &[u8]) -> Option<Vec<u8>> {
    compress_with_gain(cctx, cdict, value, 1, 8)
}

fn compress_with_gain<'a>(
    cctx: &mut CCtx<'a>,
    cdict: &'a CDict<'_>,
    value: &[u8],
    num: usize,
    den: usize,
) -> Option<Vec<u8>> {
    cctx.ref_cdict(cdict).ok()?;
    let mut out = Vec::with_capacity(zstd::zstd_safe::compress_bound(value.len()));
    let n = cctx.compress2(&mut &mut out, value).ok()?;
    worthwhile_ratio(value.len(), n, num, den).then_some(out)
}

fn compress_lz4_with_gain(value: &[u8], num: usize, den: usize) -> Option<Vec<u8>> {
    let out = lz4_flex::block::compress(value);
    worthwhile_ratio(value.len(), out.len(), num, den).then_some(out)
}

/// Savings must cover the packed header as well as the minimum gain (`num / den` of original).
#[cfg(test)]
fn worthwhile(original: usize, compressed: usize) -> bool {
    worthwhile_ratio(original, compressed, 1, 8)
}

fn worthwhile_ratio(original: usize, compressed: usize, num: usize, den: usize) -> bool {
    let packed = compressed.saturating_add(PACKED_BYTES);
    let min_saved = original.saturating_mul(num) / den.max(1);
    packed.saturating_add(min_saved) < original
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
            registry: Arc::new(DictRegistry::default()),
            samplers: Mutex::new(HashMap::new()),
            enabled: AtomicBool::new(enabled),
            algorithm: AtomicU8::new(CompressionAlgo::Auto as u8),
            min_idle_secs: AtomicU64::new(min_idle_secs),
            min_size: AtomicUsize::new(min_size),
            pack_max_bytes: AtomicU64::new(DEFAULT_PACK_BYTES),
            pack_budget_ms: AtomicU64::new(DEFAULT_PACK_BUDGET_MS),
            sample_hasher: RandomState::new(),
            adaptive_dicts: AtomicBool::new(true),
            adaptive_maintenance: AtomicBool::new(true),
            min_ttl_secs: AtomicU64::new(2),
            hot_max_bytes: AtomicU64::new(0),
            latency_target_us: AtomicU64::new(DEFAULT_LATENCY_TARGET_US),
            pressure_until_ms: AtomicU64::new(0),
            latency_ring: std::array::from_fn(|_| AtomicU64::new(0)),
            latency_cursor: AtomicU32::new(0),
            latency_count: AtomicU64::new(0),
            stats: CodecStats::default(),
        }
    }

    pub fn registry(&self) -> Arc<DictRegistry> {
        Arc::clone(&self.registry)
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Relaxed)
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Relaxed);
    }

    pub fn algorithm(&self) -> CompressionAlgo {
        CompressionAlgo::from_u8(self.algorithm.load(Relaxed))
    }

    pub fn set_algorithm(&self, algo: CompressionAlgo) {
        self.algorithm.store(algo as u8, Relaxed);
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
    pub fn latency_target_us(&self) -> u64 {
        self.latency_target_us.load(Relaxed)
    }
    pub fn set_latency_target_us(&self, us: u64) {
        self.latency_target_us.store(us.max(1), Relaxed);
    }

    pub fn ttl_eligible(&self, e: &Entry, now_ms: u64) -> bool {
        let at = e.expire_at();
        at == 0 || at.saturating_sub(now_ms) >= self.min_ttl_secs().saturating_mul(1000)
    }

    /// Sampled command execution time, excluding socket reads/writes; updates rolling P95/P99 window.
    pub fn observe_request(&self, elapsed_us: u64, now_ms: u64) {
        let slot = (self.latency_cursor.fetch_add(1, Relaxed) as usize) % LATENCY_WINDOW;
        self.latency_ring[slot].store(elapsed_us, Relaxed);
        self.latency_count.fetch_add(1, Relaxed);
        let target = self.latency_target_us();
        if elapsed_us >= target {
            self.pressure_until_ms
                .fetch_max(now_ms.saturating_add(1000), Relaxed);
        } else if self.latency_count.load(Relaxed) >= 20 {
            let (p95, p99) = self.latency_percentiles_us();
            if p99 >= target || p95 >= target.saturating_mul(3) / 4 {
                self.pressure_until_ms
                    .fetch_max(now_ms.saturating_add(1000), Relaxed);
            }
        }
    }

    /// Rolling `(p95_us, p99_us)` from the last `LATENCY_WINDOW` sampled commands.
    pub fn latency_percentiles_us(&self) -> (u64, u64) {
        let n = (self.latency_count.load(Relaxed) as usize).min(LATENCY_WINDOW);
        if n == 0 {
            return (0, 0);
        }
        let mut samples = [0u64; LATENCY_WINDOW];
        for (i, slot) in self.latency_ring.iter().take(n).enumerate() {
            samples[i] = slot.load(Relaxed);
        }
        let slice = &mut samples[..n];
        slice.sort_unstable();
        let p95_idx = ((n * 95).saturating_sub(1) / 100).min(n - 1);
        let p99_idx = ((n * 99).saturating_sub(1) / 100).min(n - 1);
        (slice[p95_idx], slice[p99_idx])
    }

    pub fn under_pressure(&self, now_ms: u64) -> bool {
        self.adaptive_maintenance() && now_ms < self.pressure_until_ms.load(Relaxed)
    }

    pub fn has_dicts(&self) -> bool {
        !self.registry.by_prefix.read().is_empty()
    }

    /// Whether the background packer can do useful work (active dicts exist, retired dicts need
    /// migration, or adaptive/LZ4 mode can compress dictionary-free values).
    pub fn can_pack(&self) -> bool {
        self.algorithm() != CompressionAlgo::Zstd || self.has_dicts() || self.has_retired_dicts()
    }

    /// Whether a plain entry with `(key, value_len)` is ready to be evaluated by the background packer.
    pub fn can_pack_entry(&self, key: &[u8], value_len: usize, now_secs: u32) -> bool {
        if value_len < self.min_size() {
            return false;
        }
        match self.algorithm() {
            CompressionAlgo::Lz4 => true,
            CompressionAlgo::Zstd => self.dict_for_key(key).is_some(),
            CompressionAlgo::Auto => {
                self.dict_for_key(key).is_some()
                    || value_len >= LARGE_LZ4_THRESHOLD
                    || !self.wants_samples(key, now_secs)
            }
        }
    }

    pub fn has_retired_dicts(&self) -> bool {
        self.registry
            .slots
            .read()
            .iter()
            .flatten()
            .any(|d| d.is_retired() && d.active_key_count() > 0)
    }

    pub fn is_dict_retired(&self, id: u32) -> bool {
        if id == LZ4_DICT_ID {
            return false;
        }
        self.dict(id).is_some_and(|d| d.is_retired())
    }

    /// Whether a retired dictionary's remaining values no longer cover its resident memory footprint
    /// and should be unpacked to plain if they cannot migrate to a new dictionary or LZ4.
    pub fn should_unpack_retired(&self, id: u32) -> bool {
        if id == LZ4_DICT_ID {
            return false;
        }
        self.dict(id)
            .is_some_and(|d| d.is_retired() && d.net_saved_bytes() < 0)
    }

    pub fn dicts_reclaimed(&self) -> u64 {
        self.registry.reclaimed.load(Relaxed)
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

    /// All live dictionaries currently resident in memory (both active and retired-with-dependents).
    pub fn dicts(&self) -> Vec<Arc<Dict>> {
        self.registry
            .slots
            .read()
            .iter()
            .filter_map(|s| s.clone())
            .collect()
    }

    /// Active (non-retired) dictionaries currently used for new compression per prefix.
    pub fn active_dicts(&self) -> Vec<Arc<Dict>> {
        self.dicts()
            .into_iter()
            .filter(|d| !d.is_retired())
            .collect()
    }

    /// Retired dictionary versions still held in memory because existing packed entries reference them.
    pub fn retired_dicts(&self) -> Vec<Arc<Dict>> {
        self.dicts()
            .into_iter()
            .filter(|d| d.is_retired())
            .collect()
    }

    pub fn dict(&self, id: u32) -> Option<Arc<Dict>> {
        self.registry.get(id)
    }

    pub fn dict_for_key(&self, key: &[u8]) -> Option<Arc<Dict>> {
        self.registry.by_prefix.read().get(prefix_of(key)).cloned()
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
        let start_inst = Instant::now();
        let res = if p.is_lz4() {
            SCRATCH.with(|scratch| {
                let mut buf = scratch.borrow_mut();
                buf.clear();
                let len = p.original_len as usize;
                buf.resize(len, 0);
                let n = lz4_flex::block::decompress_into(stored, &mut buf[..len])
                    .map_err(|_| Corrupt)?;
                if n != len {
                    return Err(Corrupt);
                }
                Ok(f(&buf[..len]))
            })
        } else {
            let dict = self.dict(p.dict).ok_or(Corrupt)?;
            SCRATCH.with(|scratch| {
                let mut buf = scratch.borrow_mut();
                buf.clear();
                buf.reserve(p.original_len as usize);
                let mut d = decompressor();
                d.ref_ddict(&dict.ddict).map_err(|_| Corrupt)?;
                let n = d.decompress(&mut &mut *buf, stored).map_err(|_| Corrupt)?;
                if n != p.original_len as usize {
                    return Err(Corrupt);
                }
                Ok(f(&buf))
            })
        };
        if res.is_ok() {
            self.stats.decompress_calls.fetch_add(1, Relaxed);
            self.stats
                .decompress_micros
                .fetch_add(start_inst.elapsed().as_micros() as u64, Relaxed);
        }
        res
    }

    /// Append exactly one value without a scratch-to-output copy. On any error, restore the
    /// previous initialized output; Cursor delegates initialized-length tracking to zstd-safe.
    pub fn append_packed(
        &self,
        p: Packed,
        stored: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<(), Corrupt> {
        let start_inst = Instant::now();
        let start = out.len();
        let orig_len = p.original_len as usize;
        if p.is_lz4() {
            out.resize(start + orig_len, 0);
            match lz4_flex::block::decompress_into(stored, &mut out[start..start + orig_len]) {
                Ok(n) if n == orig_len => {
                    self.stats.decompress_calls.fetch_add(1, Relaxed);
                    self.stats
                        .decompress_micros
                        .fetch_add(start_inst.elapsed().as_micros() as u64, Relaxed);
                    return Ok(());
                }
                _ => {
                    out.truncate(start);
                    return Err(Corrupt);
                }
            }
        }
        let dict = self.dict(p.dict).ok_or(Corrupt)?;
        out.reserve(orig_len);
        let result = {
            let mut d = decompressor();
            d.ref_ddict(&dict.ddict).and_then(|_| {
                let mut destination = std::io::Cursor::new(&mut *out);
                destination.set_position(start as u64);
                d.decompress(&mut destination, stored)
            })
        };
        if result.ok() != Some(orig_len) {
            out.truncate(start);
            return Err(Corrupt);
        }
        self.stats.decompress_calls.fetch_add(1, Relaxed);
        self.stats
            .decompress_micros
            .fetch_add(start_inst.elapsed().as_micros() as u64, Relaxed);
        Ok(())
    }

    /// Compresses `value` with `dict`. Returns `None` when it would not save at least 1/8 of the size.
    pub fn compress(&self, dict: impl AsRef<Dict>, value: &[u8]) -> Option<Vec<u8>> {
        let dict = dict.as_ref();
        let mut c = compressor();
        let out = compress_with(&mut c, &dict.cdict, value);
        if dict.observe_eval(value.len(), out.as_ref().map(Vec::len)) {
            self.stats.dict_drifts.fetch_add(1, Relaxed);
        }
        out
    }

    /// Compresses `value` with dictionary-free LZ4 block compression if it saves at least 1/8.
    pub fn compress_lz4(&self, value: &[u8]) -> Option<Vec<u8>> {
        compress_lz4_with_gain(value, 1, 8)
    }

    /// Builds the compressed replacement for a plain entry using `dict`, if worthwhile.
    pub fn pack(&self, dict: impl AsRef<Dict>, e: &Entry) -> Option<Entry> {
        let dict = dict.as_ref();
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

    /// Builds an LZ4-compressed replacement for a plain entry, if worthwhile.
    pub fn pack_lz4(&self, e: &Entry) -> Option<Entry> {
        let value = e.plain()?;
        let compressed = self.compress_lz4(value)?;
        Some(Entry::new_packed(
            e.key(),
            &compressed,
            e.expire_at(),
            e.meta,
            Packed::lz4(value.len() as u32),
        ))
    }

    /// Probes an active dictionary on a plain sample value to detect schema/distribution drift.
    pub fn probe_drift(&self, key: &[u8], value: &[u8]) -> bool {
        self.probe_drift_prefix(prefix_of(key), value)
    }

    /// Probes an active dictionary by prefix on a plain sample value outside the shard lock.
    pub fn probe_drift_prefix(&self, prefix: &[u8], value: &[u8]) -> bool {
        let Some(dict) = self.registry.by_prefix.read().get(prefix).cloned() else {
            return false;
        };
        if dict.is_drifted() || dict.is_retired() {
            return true;
        }
        let mut c = compressor();
        let out = compress_with(&mut c, &dict.cdict, value);
        if dict.observe_eval(value.len(), out.as_ref().map(Vec::len)) {
            self.stats.dict_drifts.fetch_add(1, Relaxed);
            return true;
        }
        false
    }

    /// Adaptive cost-benefit selector: chooses `(dict_id, compressed_bytes)` among Zstd dictionary,
    /// LZ4 block (`LZ4_DICT_ID`), or `None` (keep plain) based on value size, LFU access frequency,
    /// dictionary availability, memory pressure, and P95/P99 latency pressure.
    #[allow(clippy::too_many_arguments)]
    pub fn compress_adaptive(
        &self,
        key: &[u8],
        value: &[u8],
        lfu: u8,
        policy_is_lfu: bool,
        memory_pressure: bool,
        now_secs: u32,
        now_ms: u64,
    ) -> Option<(u32, Vec<u8>)> {
        if value.len() < self.min_size() {
            return None;
        }
        let latency_pressure = self.under_pressure(now_ms) && !memory_pressure;
        let is_hot = policy_is_lfu && lfu >= HOT_SKIP_LFU_THRESHOLD;
        let is_warm = policy_is_lfu && lfu >= WARM_LFU_THRESHOLD;

        // Under latency pressure (or when hot_max_bytes is active), very hot LFU keys stay plain.
        if is_hot && (latency_pressure || self.hot_max_bytes() > 0) && !memory_pressure {
            self.stats.cost_benefit_skips.fetch_add(1, Relaxed);
            return None;
        }

        // Minimum required savings ratio (num / den of original length, net of PACKED_BYTES):
        // - Under memory pressure: 1/10 (10%) to maximize saved bytes.
        // - Under latency pressure for Zstd: 1/4 (25%) so marginal Zstd gains don't hurt P95/P99.
        // - Normal operation: 1/8 (12.5%).
        let (zstd_num, zstd_den) = if memory_pressure {
            (1, 10)
        } else if latency_pressure {
            (1, 4)
        } else {
            (1, 8)
        };
        let (lz4_num, lz4_den) = if memory_pressure { (1, 10) } else { (1, 8) };

        match self.algorithm() {
            CompressionAlgo::Lz4 => {
                let res = compress_lz4_with_gain(value, lz4_num, lz4_den).map(|c| (LZ4_DICT_ID, c));
                if res.is_none() {
                    self.stats.cost_benefit_skips.fetch_add(1, Relaxed);
                }
                return res;
            }
            CompressionAlgo::Zstd => {
                let dict = self.dict_for_key(key)?;
                let mut c = compressor();
                let out = compress_with_gain(&mut c, &dict.cdict, value, zstd_num, zstd_den);
                if out.is_none() {
                    self.stats.cost_benefit_skips.fetch_add(1, Relaxed);
                }
                return out.map(|c| (dict.id, c));
            }
            CompressionAlgo::Auto => {}
        }

        let dict = self.dict_for_key(key);

        // 1. Warm LFU keys or latency pressure: prefer fast LZ4 if it achieves worthwhile savings.
        if (is_warm || latency_pressure) && !memory_pressure {
            if let Some(lz4) = compress_lz4_with_gain(value, lz4_num, lz4_den) {
                return Some((LZ4_DICT_ID, lz4));
            }
            if latency_pressure && is_warm {
                self.stats.cost_benefit_skips.fetch_add(1, Relaxed);
                return None;
            }
        }

        // 2. Large values (>= 4 KiB): LZ4 compresses well without a dictionary and decompresses
        //    much faster than Zstd. Pick LZ4 unless a trained Zstd dictionary beats it by > 20%.
        if value.len() >= LARGE_LZ4_THRESHOLD && !memory_pressure {
            let lz4 = compress_lz4_with_gain(value, lz4_num, lz4_den);
            let zstd = dict.as_ref().and_then(|d| {
                let mut c = compressor();
                let out = compress_with_gain(&mut c, &d.cdict, value, zstd_num, zstd_den);
                out.map(|bytes| (d.id, bytes))
            });
            return match (lz4, zstd) {
                (Some(l), Some((id, z))) => {
                    let l_saved = value.len().saturating_sub(l.len() + PACKED_BYTES);
                    let z_saved = value.len().saturating_sub(z.len() + PACKED_BYTES);
                    if l_saved.saturating_mul(5) >= z_saved.saturating_mul(4) {
                        Some((LZ4_DICT_ID, l))
                    } else {
                        Some((id, z))
                    }
                }
                (Some(l), None) => Some((LZ4_DICT_ID, l)),
                (None, Some(z)) => Some(z),
                (None, None) => {
                    self.stats.cost_benefit_skips.fetch_add(1, Relaxed);
                    None
                }
            };
        }

        // 3. Cold entry with an active prefix dictionary: try Zstd dictionary first, fall back to LZ4.
        if let Some(d) = dict {
            let mut c = compressor();
            let zstd = compress_with_gain(&mut c, &d.cdict, value, zstd_num, zstd_den);
            if let Some(z) = zstd {
                return Some((d.id, z));
            }
            if let Some(l) = compress_lz4_with_gain(value, lz4_num, lz4_den) {
                return Some((LZ4_DICT_ID, l));
            }
            self.stats.cost_benefit_skips.fetch_add(1, Relaxed);
            return None;
        }

        // 4. No dictionary for this prefix: if dictionary training is not pending for this prefix
        //    (e.g. rejected prefix or MAX_DICTS full), compress with dictionary-free LZ4 if worthwhile.
        if !self.wants_samples(key, now_secs) {
            if let Some(l) = compress_lz4_with_gain(value, lz4_num, lz4_den) {
                return Some((LZ4_DICT_ID, l));
            }
            self.stats.cost_benefit_skips.fetch_add(1, Relaxed);
        }
        None
    }

    /// Retires any active dictionary that has active packed entries yet fails to amortize its
    /// prepared table memory (`net_saved_bytes() < 0`). Returns the number of retired dictionaries.
    /// Existing entries on retired dictionaries stay readable and can be migrated to LZ4 or plain.
    pub fn retire_unprofitable_dicts(&self) -> usize {
        let mut by_prefix = self.registry.by_prefix.write();
        let unprofitable: Vec<Box<[u8]>> = by_prefix
            .iter()
            .filter(|(_, d)| d.active_key_count() > 0 && d.net_saved_bytes() < 0)
            .map(|(p, _)| p.clone())
            .collect();
        let mut count = 0;
        for prefix in unprofitable {
            if let Some(old) = by_prefix.remove(&prefix) {
                old.retired.store(true, SeqCst);
                count += 1;
                if old.active_key_count() == 0 {
                    self.registry.try_reclaim_slot(old.id);
                }
            }
        }
        count
    }

    /// Whether values with this key should still be sampled for dictionary training or retraining.
    pub fn wants_samples(&self, key: &[u8], now_secs: u32) -> bool {
        if self.algorithm() == CompressionAlgo::Lz4 {
            return false;
        }
        let prefix = prefix_of(key);
        let active_count = {
            let by_prefix = self.registry.by_prefix.read();
            if let Some(dict) = by_prefix.get(prefix) {
                if !dict.is_drifted() {
                    return false;
                }
                0 // Retraining an existing prefix replaces its own active slot.
            } else {
                if by_prefix.len() >= MAX_DICTS {
                    return false;
                }
                by_prefix.len()
            }
        };
        let samplers = self.samplers.lock();
        match samplers.get(prefix) {
            Some(s) => !s.training && s.rejected_until <= now_secs,
            None => {
                samplers.len() + active_count < MAX_DICTS
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
        if self.algorithm() == CompressionAlgo::Lz4 {
            return None;
        }
        let active_count = {
            let by_prefix = self.registry.by_prefix.read();
            if let Some(dict) = by_prefix.get(prefix) {
                if !dict.is_drifted() {
                    return None;
                }
                0
            } else {
                if by_prefix.len() >= MAX_DICTS {
                    return None;
                }
                by_prefix.len()
            }
        };
        let mut samplers = self.samplers.lock();
        if !samplers.contains_key(prefix) && samplers.len() + active_count >= MAX_DICTS {
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
    /// `MIN_GAIN`. If the prefix already had a dictionary, the previous version is marked `retired`
    /// and kept alive until all entries referencing it are migrated or removed.
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
        if let Some(existing) = self.registry.by_prefix.read().get(&prefix) {
            existing.drifted.store(false, Relaxed);
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
        let existing = self.registry.by_prefix.read().get(prefix).cloned();
        let mut best = None;
        let mut best_cost = usize::MAX;
        let mut best_ratio: f64 = 1.0;
        let original: usize = test.iter().map(Vec::len).sum();
        let existing_cost = existing.as_ref().map(|old| {
            let mut cctx = compressor();
            let mut physical = 0usize;
            for value in test {
                if let Some(packed) = compress_with(&mut cctx, &old.cdict, value) {
                    physical += packed.len() + PACKED_BYTES;
                } else {
                    physical += value.len();
                }
            }
            let pop_cost = physical.saturating_mul(samples.len()) / test.len().max(1);
            old.prepared_bytes.saturating_add(pop_cost)
        });
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
                            physical += packed.len() + PACKED_BYTES;
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
                let beats_existing = existing_cost.is_none_or(|old_cost| cost < old_cost);
                if ratio >= MIN_GAIN
                    && (!adaptive || cost < population_plain)
                    && beats_existing
                    && cost < best_cost
                {
                    best_cost = cost;
                    best = Some((raw.len(), level, ratio, prepared, cdict, ddict));
                }
            }
        }
        let Some((raw_bytes, level, ratio, prepared_bytes, cdict, ddict)) = best else {
            return TrainOutcome::Rejected { ratio: best_ratio };
        };
        let mut by_prefix = self.registry.by_prefix.write();
        let prev = by_prefix.get(prefix).cloned();
        if prev.is_none() && by_prefix.len() >= MAX_DICTS {
            return TrainOutcome::Failed;
        }
        let version = prev.as_ref().map_or(1, |d| d.version + 1);
        let id = {
            let mut slots = self.registry.slots.write();
            let id = slots.len() as u32;
            if id == LZ4_DICT_ID {
                return TrainOutcome::Failed;
            }
            let dict = Arc::new(Dict {
                id,
                version,
                prefix: prefix.into(),
                trained_ratio: ratio,
                prepared_bytes,
                raw_bytes,
                level,
                active_keys: AtomicUsize::new(0),
                live_original_bytes: AtomicU64::new(0),
                live_stored_bytes: AtomicU64::new(0),
                retired: AtomicBool::new(false),
                drifted: AtomicBool::new(false),
                recent_evals: AtomicU64::new(0),
                recent_original_bytes: AtomicU64::new(0),
                recent_stored_bytes: AtomicU64::new(0),
                recent_rejects: AtomicU64::new(0),
                cdict,
                ddict,
            });
            slots.push(Some(Arc::clone(&dict)));
            by_prefix.insert(prefix.into(), dict);
            id
        };
        if let Some(old) = prev {
            old.retired.store(true, SeqCst);
            self.stats.dicts_retrained.fetch_add(1, Relaxed);
            if old.active_key_count() == 0 {
                self.registry.try_reclaim_slot(old.id);
            }
        }
        TrainOutcome::Accepted { id, ratio }
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

    /// Second schema with completely different keys/tokens to exercise drift detection and retraining.
    pub(crate) fn drifted_sample_values(n: usize, seed: u64) -> Vec<Vec<u8>> {
        let mut s = seed | 1;
        let mut next = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let warehouses = ["GRU-NORTH", "VCP-CENTRAL", "GIG-COAST", "CNF-INLAND"];
        let carriers = ["EXPRESS-AIR", "FREIGHT-RAIL", "LOCAL-VAN", "CARGO-SEA"];
        (0..n)
            .map(|_| {
                let a = next();
                let b = next();
                format!(
                    r#"<shipment manifest_code="SHP-{:08}" origin_facility="{}" carrier_mode="{}" customs_status="CLEARED-DOMESTIC" pallet_weight_grams="{}" insurance_policy="POL-{:012X}" hazmat_class="NONE-STANDARD"/>"#,
                    a % 100_000_000,
                    warehouses[(a % 4) as usize],
                    carriers[(b % 4) as usize],
                    10_000 + (b % 900_000),
                    a.wrapping_mul(b) & 0xFFFF_FFFF_FFFF
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
            let packed = codec.pack(&dict, &e).expect("compressible");
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
            .pack(&dict, &Entry::new(b"session:k", &expected, 0, 0))
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
    use super::tests::{drifted_sample_values, sample_values};
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
                        .compress(&d, v)
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
            .pack(&dict, &Entry::new(b"session:test", &value, 0, 0))
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

    #[test]
    fn adaptive_selection_switches_between_zstd_lz4_and_plain() {
        let codec = Codec::new(true, 0, 64);
        let samples = sample_values(1000, 11);
        assert!(matches!(
            codec.train(b"session:".as_slice().into(), samples.clone(), 0),
            TrainOutcome::Accepted { id: 0, .. }
        ));
        let v = &samples[0];

        // Cold LFU entry with active prefix dictionary -> Zstd (dict 0).
        let (dict_id, zstd_bytes) = codec
            .compress_adaptive(b"session:1", v, 5, true, false, 10, 1000)
            .unwrap();
        assert_eq!(dict_id, 0);

        // Warm LFU entry with compressible payload -> LZ4 (LZ4_DICT_ID).
        let warm_payload = v.repeat(4);
        let (warm_id, lz4_bytes) = codec
            .compress_adaptive(b"session:warm", &warm_payload, 16, true, false, 10, 1000)
            .unwrap();
        assert_eq!(warm_id, LZ4_DICT_ID);
        assert!(lz4_bytes.len() < warm_payload.len());

        // Round-trip LZ4 through both with_packed_value and append_packed.
        let p = Packed::lz4(warm_payload.len() as u32);
        assert_eq!(
            codec
                .with_packed_value(p, &lz4_bytes, |b| b.to_vec())
                .unwrap(),
            warm_payload
        );
        let mut out = b"$header\r\n".to_vec();
        codec.append_packed(p, &lz4_bytes, &mut out).unwrap();
        assert_eq!(&out[9..], warm_payload.as_slice());

        // Hot LFU entry under latency pressure -> skipped (Plain / None).
        codec.observe_request(2500, 2000);
        assert!(
            codec
                .compress_adaptive(b"session:hot", &warm_payload, 30, true, false, 10, 2050)
                .is_none()
        );
        assert!(codec.stats.cost_benefit_skips.load(Relaxed) >= 1);
        assert!(!zstd_bytes.is_empty());
    }

    #[test]
    fn dynamic_dictionary_retrains_on_drift_and_retains_old_version_until_unreferenced() {
        let codec = Codec::new(true, 0, 64);
        let v1 = sample_values(1000, 21);
        let TrainOutcome::Accepted { id: id1, .. } =
            codec.train(b"item:".as_slice().into(), v1.clone(), 0)
        else {
            panic!("v1 training failed");
        };
        assert_eq!(id1, 0);
        let d1 = codec.dict(id1).unwrap();
        assert_eq!(d1.version, 1);

        // Simulate one stored entry referencing dictionary v1.
        let packed_v1 = codec
            .pack(&d1, &Entry::new(b"item:old", &v1[0], 0, 0))
            .unwrap();
        codec
            .registry()
            .inc_ref(id1, 1, v1[0].len() as u64, packed_v1.stored().len() as u64);
        assert_eq!(d1.active_key_count(), 1);
        assert!(!codec.wants_samples(b"item:new", 10));

        // Feed drifted schema values until drift is detected.
        let v2 = drifted_sample_values(1000, 99);
        for sample in v2.iter().take(DRIFT_WINDOW as usize) {
            codec.probe_drift(b"item:new", sample);
        }
        assert!(d1.is_drifted());
        assert_eq!(codec.stats.dict_drifts.load(Relaxed), 1);
        assert!(codec.wants_samples(b"item:new", 10));

        // Retrain on v2: v2 becomes active (id 1, version 2), while v1 is retired but kept alive!
        let TrainOutcome::Accepted { id: id2, .. } =
            codec.train(b"item:".as_slice().into(), v2.clone(), 20)
        else {
            panic!("v2 retraining failed");
        };
        assert_eq!(id2, 1);
        assert_eq!(codec.stats.dicts_retrained.load(Relaxed), 1);
        let d2 = codec.dict_for_key(b"item:any").unwrap();
        assert_eq!((d2.id, d2.version), (id2, 2));
        assert!(codec.is_dict_retired(id1));
        assert_eq!(codec.active_dicts().len(), 1);
        assert_eq!(codec.retired_dicts().len(), 1);

        // Old entry packed with v1 still decompresses byte-for-byte!
        assert_eq!(codec.with_value(&packed_v1, |b| b.to_vec()).unwrap(), v1[0]);

        // Once the last v1 reference is released, v1 is automatically reclaimed!
        codec
            .registry()
            .dec_ref(id1, 1, v1[0].len() as u64, packed_v1.stored().len() as u64);
        assert!(codec.dict(id1).is_none());
        assert_eq!(codec.retired_dicts().len(), 0);
        assert_eq!(codec.dicts_reclaimed(), 1);
    }

    #[test]
    fn cost_benefit_tracks_p95_p99_and_retires_unprofitable_dictionaries() {
        let codec = Codec::new(true, 0, 64);
        for i in 1..=64 {
            codec.observe_request(i * 10, 100);
        }
        let (p95, p99) = codec.latency_percentiles_us();
        assert!(p95 >= 600 && p99 >= 630, "p95={p95} p99={p99}");

        let v1 = sample_values(1000, 55);
        let TrainOutcome::Accepted { id, .. } =
            codec.train(b"tiny:".as_slice().into(), v1.clone(), 0)
        else {
            panic!("training failed");
        };
        // Only 2 keys compressed -> gross savings (~400 B) < prepared_bytes (~20 KB), net negative.
        codec.registry().inc_ref(id, 2, 600, 200);
        assert!(codec.dict(id).unwrap().net_saved_bytes() < 0);
        assert_eq!(codec.retire_unprofitable_dicts(), 1);
        assert!(codec.dict_for_key(b"tiny:1").is_none());
        assert!(codec.is_dict_retired(id));
        codec.registry().dec_ref(id, 2, 600, 200);
        assert!(codec.dict(id).is_none());
    }
}
