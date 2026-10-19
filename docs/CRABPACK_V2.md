# CrabPack v2: Adaptive Compression, Dynamic Dictionaries & Cost-Benefit Control

CrabPack v2 evolves CrabCache's transparent value compaction subsystem along three pillars:

1. **Adaptive Compression (`auto` | `zstd` | `lz4`)** — selects Zstd dictionary compression, dictionary-free LZ4 block compression, or uncompressed `Plain` storage per key based on value size, LFU access frequency, dictionary availability, and achieved memory savings.
2. **Dynamic Dictionaries** — detects schema drift on existing prefixes, retrains a new dictionary version (`version=2, 3, ...`) in the background, keeps retired versions resident while compressed entries still depend on them, and automatically reclaims retired dictionary slots as soon as their reference count (`active_keys`) reaches `0`.
3. **Cost-Benefit Controller** — accounts for **real net memory savings** (subtracting `PACKED_BYTES` headers, resident dictionary `prepared_bytes`, sampler buffers, and shard access/hot metadata), tracks a sliding window of P95/P99 command latencies (`compression_observed_p95_us` / `compression_observed_p99_us`) and decompression CPU time (`compression_decompress_us`), and automatically retires dictionaries whose net savings stay negative.

---

## 1. Architecture & Configuration

### CLI Flags & Environment Variables

| Flag | Environment Variable | Default | Description |
|---|---|---|---|
| `--compression-algorithm <ALGO>` | `CRABCACHE_COMPRESSION_ALGORITHM` | `auto` | `auto` (adaptive Zstd + LZ4 + Plain), `zstd` (dictionary-only), or `lz4` (block-only) |
| `--compression-latency-target-us <US>` | `CRABCACHE_COMPRESSION_LATENCY_TARGET_US` | `500` | P95/P99 command latency target in microseconds before background compaction backs off and prefers LZ4/Plain |

All parameters are hot-reloadable at runtime via `CONFIG GET` / `CONFIG SET`:
- `CONFIG SET compression-algorithm auto|zstd|lz4`
- `CONFIG SET compression-latency-target-us <microseconds>`

### New `INFO compression` Telemetry

| Field | Meaning |
|---|---|
| `compression_algorithm` | Active algorithm selector (`auto`, `zstd`, or `lz4`) |
| `compression_dicts_active` | Currently active prefix dictionaries |
| `compression_dicts_retired` | Retired dictionary versions still referenced by live entries |
| `compression_dicts_retrained` | Cumulative dictionary retrainings triggered by drift |
| `compression_dicts_reclaimed` | Cumulative retired dictionary slots freed when `active_keys == 0` |
| `compression_dict_drifts` | Cumulative prefix schema drift detections |
| `compressed_zstd_keys` | Live keys compressed with a Zstd prefix dictionary |
| `compressed_lz4_keys` | Live keys compressed with dictionary-free LZ4 (`LZ4_DICT_ID = u32::MAX`) |
| `compression_net_memory_saved_bytes` | Net memory saved (`gross_saved - PACKED_BYTES - prepared_dict_bytes - sampler_bytes - shard_meta_bytes`) |
| `compression_cost_benefit_skips` | Evaluations skipped because compression was not cost-effective |
| `compression_decompress_calls` | Cumulative transparent decompressions on read |
| `compression_decompress_us` | Cumulative microseconds spent decompressing values on read |
| `compression_latency_target_us` | Configured P95/P99 latency target (`µs`) |
| `compression_observed_p95_us` | Sliding-window P95 command latency (`µs`) |
| `compression_observed_p99_us` | Sliding-window P99 command latency (`µs`) |
| `dictN:...` | Includes `version=<v>,active_keys=<k>,retired=<0\|1>,net_saved_bytes=<bytes>` |

---

## 2. Recorded Benchmark Results (`2026-10-19-crabpack-v2`)

Reproducible runner: [`scripts/bench-crabpack-v2.py`](../scripts/bench-crabpack-v2.py)  
Raw logs, JSON records, and SHA-256 manifest: [`docs/benchmark-results/2026-10-19-crabpack-v2/summary.json`](benchmark-results/2026-10-19-crabpack-v2/summary.json)

### Workload A: Mixed-Size & Entropy Dataset (1,800 keys, `1,573,226 B` raw payload, 3 repeats median)

- **1,500 small structured keys** (`session:0..1499`, ~195 B JSON each): share cross-key JSON structure; require a trained Zstd dictionary.
- **200 large structured blobs** (`blob:0..199`, ~6.2 KiB each): contain intra-value repetition; compress well with fast dictionary-free LZ4.
- **100 high-entropy random values** (`session:rnd_0..99`, 512 B each): incompressible noise; must be rejected by the cost-benefit gate.

| Mode | Entry Memory (`used_memory`) | Prepared Dicts (`prepared_dict_bytes`) | **Total Footprint** | **Net Memory Saved** | Zstd Keys | LZ4 Keys | Cost-Benefit Skips | Decompress CPU (`µs/call`) | Large `GET` Throughput (`ops/s`) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `plain` | 1,664,996 B | 0 B | 1,664,996 B | 0 B | 0 | 0 | 0 | 0.000 µs | 187,965.9 |
| `lz4` | 533,946 B | 0 B | 533,946 B | +1,131,050 B | 0 | 200 | 1,600 | 0.565 µs | 145,488.7 |
| `zstd` | 237,062 B | 158,886 B (2 dicts) | 395,948 B | +1,267,512 B | 1,700 | 0 | 100 | 0.449 µs | 138,953.2 |
| **`auto` (v2)** | **250,595 B** | **107,936 B (1 dict)** | **358,531 B** | **+1,257,798 B** | **1,500** | **200** | **100** | **0.294 µs** | **148,089.0** |

**Key Takeaways:**
- **`auto` achieves the lowest total memory footprint (`358.5 KiB`)** — **78.5% smaller than `plain` (`1.59 MiB`)** and **9.4% smaller total footprint than pure `zstd` (`395.9 KiB`)** because it trains a Zstd dictionary only for the 1,500 small `session:*` JSONs while routing the 200 large `blob:*` values to dictionary-free LZ4 (saving `50,950 B` of prepared Zstd dictionary tables).
- **`auto` reduces average decompression CPU time per call by 34.5% vs pure `zstd`** (`0.294 µs/call` vs `0.449 µs/call`) and improves large-payload `GET` throughput (`148.1k ops/s` vs `139.0k ops/s`).
- All 100 incompressible random values are rejected by `compression_cost_benefit_skips`.

### Workload B: Dynamic Dictionary Drift, Version Coexistence & Slot Reclamation (2,200 keys, 3 repeats median)

1. **Phase 1 (Drift & Version Coexistence):** 1,100 `order:0..1099` keys (JSON v1) train `dict0` (`version=1`, `6.82x` ratio). Inserting 1,100 `order:v2_0..1099` keys (XML v2) under the same `order:` prefix triggers drift detection (`compression_dict_drifts = 1`), trains `dict1` (`version=2`) in the background, and marks `dict0` as retired (`compression_dicts_active = 1`, `compression_dicts_retired = 1`). All 2,200 keys remain compressed and readable across both dictionary versions.
2. **Phase 2 (Migration & Automatic Slot Reclamation):** Updating `order:0..1099` to the v2 schema drops `dict0`'s `active_keys` reference count to `0`, immediately freeing the retired dictionary slot (`compression_dicts_reclaimed = 1`, `compression_dicts_retired = 0`) with `6.66x` post-migration ratio and `+280,516 B` net memory saved.

---

## 3. Reproducing Validation & Benchmarks

```bash
# Run unit + integration tests (including CrabPack v2 end-to-end suite)
cargo nextest run --release

# Run CrabPack v2 benchmark suite and verify SHA-256 manifest + invariants
python3 scripts/bench-crabpack-v2.py --out docs/benchmark-results/2026-10-19-crabpack-v2 --repeats 3
python3 scripts/bench-crabpack-v2.py --check docs/benchmark-results/2026-10-19-crabpack-v2
```
