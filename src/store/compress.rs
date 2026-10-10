//! CrabPack: transparent compression of cold values with dictionaries trained per key prefix.
//!
//! Cache values are mostly small (100 B to a few KB) and individually compress poorly, but values that
//! share a key prefix (`user:`, `session:`) tend to share structure: field names, formats, enums. A zstd
//! dictionary trained on samples from one prefix captures that shared structure, so each value can be
//! compressed on its own. Measured on realistic JSON (sessions, products, API responses of ~300 B):
//! plain zstd 1.2-1.5x, zstd with a 16 KB trained dictionary 3.2-3.6x.
//!
//! The flow, driven by a background task (`Db::pack_cycle`):
//! 1. Random entries are sampled and grouped by prefix.
//! 2. Once a prefix has enough samples, a dictionary is trained off the I/O threads and kept only if it
//!    compresses held-out samples by at least `MIN_GAIN`.
//! 3. Entries idle for `min_idle` seconds whose prefix has a dictionary are compressed in place.
//!
//! Reads decompress transparently: clients always get the exact bytes they wrote. Any write stores the
//! value plain again, so hot keys stay uncompressed.

use super::entry::{Entry, Packed};
use parking_lot::{Mutex, RwLock};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::Relaxed};
use zstd::zstd_safe::{CCtx, CDict, CParameter, DCtx, DDict, DParameter, FrameFormat};

/// Maximum dictionary size. 16 KB was the knee of the curve on the measured datasets.
pub const DICT_SIZE: usize = 16 * 1024;
/// Samples gathered per prefix before training.
pub const SAMPLES_PER_PREFIX: usize = 1000;
/// Training also starts once this many sample bytes are collected.
pub const SAMPLE_BYTES_PER_PREFIX: usize = 256 * 1024;
/// A dictionary must shrink held-out samples by at least this ratio to be kept.
pub const MIN_GAIN: f64 = 1.25;
/// Upper bound on dictionaries (each costs ~16 KB plus prepared zstd tables).
pub const MAX_DICTS: usize = 64;
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
    (n + n / 8 < value.len()).then_some(out)
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
        let dict = self.dict(p.dict).ok_or(Corrupt)?;
        SCRATCH.with(|scratch| {
            let mut buf = scratch.borrow_mut();
            buf.clear();
            buf.reserve(p.original_len as usize);
            let n = DCTX
                .with(|d| {
                    let mut d = d.borrow_mut();
                    d.ref_ddict(&dict.ddict)?;
                    d.decompress(&mut &mut *buf, e.stored())
                })
                .map_err(|_| Corrupt)?;
            if n != p.original_len as usize {
                return Err(Corrupt);
            }
            Ok(f(&buf))
        })
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
        match self.samplers.lock().get(prefix_of(key)) {
            Some(s) => {
                !s.training && s.rejected_until <= now_secs && s.samples.len() < SAMPLES_PER_PREFIX
            }
            None => true,
        }
    }

    /// Records a sample value. Returns the prefix and its samples once it has enough to train; the
    /// caller then runs `train` off the I/O threads.
    pub fn offer_sample(&self, key: &[u8], value: &[u8], now_secs: u32) -> Option<SampleBatch> {
        let prefix = prefix_of(key);
        let mut samplers = self.samplers.lock();
        let s = samplers.entry(prefix.into()).or_default();
        if s.training || s.rejected_until > now_secs {
            return None;
        }
        s.samples.push(value.to_vec());
        s.bytes += value.len();
        if s.samples.len() >= SAMPLES_PER_PREFIX || s.bytes >= SAMPLE_BYTES_PER_PREFIX {
            s.training = true;
            s.bytes = 0;
            return Some((prefix.into(), std::mem::take(&mut s.samples)));
        }
        None
    }

    /// Trains a dictionary from `samples` (CPU heavy: run it on a blocking thread). The last fifth of
    /// the samples is held out to measure the gain; the dictionary is registered only if it reaches
    /// `MIN_GAIN`.
    pub fn train(&self, prefix: Box<[u8]>, samples: Vec<Vec<u8>>, now_secs: u32) -> TrainOutcome {
        let outcome = self.try_train(&prefix, &samples);
        let mut samplers = self.samplers.lock();
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
        let Ok(raw) = zstd::dict::from_samples(train, DICT_SIZE) else {
            return TrainOutcome::Failed;
        };
        let cdict = CDict::create(&raw, LEVEL);
        // Measure with a private context before registering anything, so a rejected dictionary is
        // simply dropped and readers are never held up by training.
        let ratio = {
            let mut cctx = CCtx::create();
            configure_compressor(&mut cctx);
            let original: usize = test.iter().map(Vec::len).sum();
            let compressed: usize = test
                .iter()
                .map(|v| compress_with(&mut cctx, &cdict, v).map_or(v.len(), |c| c.len()))
                .sum();
            original as f64 / compressed.max(1) as f64
        };
        if ratio < MIN_GAIN {
            return TrainOutcome::Rejected { ratio };
        }
        let mut by_prefix = self.by_prefix.write();
        let id = self.count.load(Relaxed);
        if id >= MAX_DICTS {
            return TrainOutcome::Failed;
        }
        let dict: &'static Dict = Box::leak(Box::new(Dict {
            id: id as u32,
            prefix: prefix.into(),
            trained_ratio: ratio,
            cdict,
            ddict: DDict::create(&raw),
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
mod tests {
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
        for _ in 0..SAMPLES_PER_PREFIX + 10 {
            if codec.offer_sample(b"k:1", v, 0).is_some() {
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
}
