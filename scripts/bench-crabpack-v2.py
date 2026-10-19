#!/usr/bin/env python3
"""Auditable local benchmark for CrabPack v2:
1. Adaptive compression (Plain vs Zstd vs LZ4 vs Auto) on a mixed workload
2. Dynamic dictionary drift detection, v2 retraining, background migration, and v1 reclamation
3. Cost-benefit net memory savings and P95/P99 latency + decompression telemetry
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import socket
import statistics
import subprocess
import time

from benchmark import Client, encode, read_resp

ROOT = Path(__file__).resolve().parents[1]


def display_path(path):
    path = Path(path).resolve()
    for base, prefix in ((ROOT.parent, '<workspace>'), (Path.home(), '<home>')):
        if path.is_relative_to(base):
            rel = path.relative_to(base).as_posix()
            return prefix if rel == '.' else f'{prefix}/{rel}'
    return path.as_posix()


def parse_info(text):
    out = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith('#') or ':' not in line:
            continue
        k, v = line.split(':', 1)
        out[k] = v
    return out


def session_json(i):
    roles = ('admin', 'member', 'viewer', 'billing')
    plans = ('free', 'pro', 'team', 'enterprise')
    regions = ('us-east-1', 'eu-west-1', 'sa-east-1', 'ap-northeast-1')
    return (
        f'{{"id":"sess_{i:010x}","user_id":{100000 + (i * 2654435761 % 900000)},'
        f'"org":"org_{i % 500:04d}","role":"{roles[i % 4]}","plan":"{plans[(i // 4) % 4]}",'
        f'"region":"{regions[(i // 16) % 4]}","active":true,"mfa":true,'
        f'"scopes":["read:items","write:items","read:profile"],'
        f'"ip":"10.{(i >> 8) & 255}.{i & 255}.1","created_at":{1700000000 + i * 3}}}'
    ).encode()


def large_blob_json(i):
    repeated = '0123456789abcdef' * 360
    return (
        f'{{"blob_id":{i},"service":"catalog-render","region":"sa-east-1",'
        f'"blocks":["alpha","beta","gamma","delta"],"payload":"{repeated}"}}'
    ).encode()


def random_token(seed, length=512):
    s = (seed ^ 0x9E3779B97F4A7C15) & 0xFFFFFFFFFFFFFFFF
    out = bytearray(length)
    for idx in range(length):
        s ^= (s >> 12) & 0xFFFFFFFFFFFFFFFF
        s ^= (s << 25) & 0xFFFFFFFFFFFFFFFF
        s ^= (s >> 27) & 0xFFFFFFFFFFFFFFFF
        out[idx] = ((s * 0x2545F4914F6CDD1D) >> 56) & 0xFF
    return bytes(out)


def drifted_xml(i):
    warehouses = ('GRU-NORTH', 'VCP-CENTRAL', 'GIG-COAST', 'CNF-INLAND')
    carriers = ('EXPRESS-AIR', 'FREIGHT-RAIL', 'LOCAL-VAN', 'CARGO-SEA')
    policy = (i * 0x517CC1B727220A95) & 0xFFFFFFFFFFFF
    return (
        f'<shipment manifest_code="SHP-{i % 100000000:08d}" '
        f'origin_facility="{warehouses[i % 4]}" '
        f'carrier_mode="{carriers[(i // 4) % 4]}" '
        f'customs_status="CLEARED-DOMESTIC" '
        f'pallet_weight_grams="{10000 + (i * 137 % 900000)}" '
        f'insurance_policy="POL-{policy:012X}" '
        f'hazmat_class="NONE-STANDARD"/>'
    ).encode()


def build_mixed_dataset(small_n=1500, large_n=200, random_n=100):
    items = []
    h = hashlib.sha256()
    for i in range(small_n):
        k = f'session:{i}'.encode()
        v = session_json(i)
        items.append((k, v))
    for i in range(large_n):
        k = f'blob:{i}'.encode()
        v = large_blob_json(i)
        items.append((k, v))
    for i in range(random_n):
        k = f'session:rnd_{i}'.encode()
        v = random_token(i + 1, 512)
        items.append((k, v))
    for k, v in items:
        h.update(len(k).to_bytes(4, 'little') + k + len(v).to_bytes(4, 'little') + v)
    return items, h.hexdigest()


def pipeline_set(client, pairs, batch_size=200):
    for start in range(0, len(pairs), batch_size):
        chunk = pairs[start:start + batch_size]
        buf = b''.join(encode(('SET', k, v)) for k, v in chunk)
        client.socket.sendall(buf)
        for _ in chunk:
            assert read_resp(client.file) == b'OK'


def verify_all(client, pairs, batch_size=200):
    for start in range(0, len(pairs), batch_size):
        chunk = pairs[start:start + batch_size]
        buf = b''.join(encode(('GET', k)) for k, _ in chunk)
        client.socket.sendall(buf)
        for _, expected in chunk:
            got = read_resp(client.file)
            assert got == expected, 'payload mismatch during verification'


def measure_gets(client, keys, rounds, pipeline=16):
    latencies_us = []
    total_ops = 0
    t0 = time.perf_counter()
    for _ in range(rounds):
        for start in range(0, len(keys), pipeline):
            chunk = keys[start:start + pipeline]
            buf = b''.join(encode(('GET', k)) for k in chunk)
            b0 = time.perf_counter()
            client.socket.sendall(buf)
            for _ in chunk:
                val = read_resp(client.file)
                assert val is not None
            elapsed_us = (time.perf_counter() - b0) * 1e6 / len(chunk)
            latencies_us.append(elapsed_us)
            total_ops += len(chunk)
    wall_s = time.perf_counter() - t0
    latencies_us.sort()
    p50 = latencies_us[len(latencies_us) // 2]
    p95 = latencies_us[min(len(latencies_us) - 1, int(len(latencies_us) * 0.95))]
    p99 = latencies_us[min(len(latencies_us) - 1, int(len(latencies_us) * 0.99))]
    return {
        'ops': total_ops,
        'ops_per_sec': round(total_ops / wall_s, 1),
        'p50_us': round(p50, 2),
        'p95_us': round(p95, 2),
        'p99_us': round(p99, 2),
    }


def start_server(binary, extra_args, log_path):
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    cmd = [
        str(binary), '--bind', '127.0.0.1', '--port', str(port),
        '--threads', '2', '--shards', '16', '--maxmemory-policy', 'allkeys-lfu',
        *extra_args,
    ]
    env = {k: v for k, v in os.environ.items() if not k.startswith('CRABCACHE_')}
    log_file = log_path.open('w')
    proc = subprocess.Popen(cmd, env=env, stdout=log_file, stderr=log_file)
    deadline = time.monotonic() + 10
    while True:
        try:
            with Client(port) as c:
                assert c.command('PING') == b'PONG'
            break
        except OSError:
            if time.monotonic() >= deadline:
                proc.terminate()
                raise
            time.sleep(0.02)
    return proc, log_file, port, [display_path(binary), *cmd[1:]]


def run_mode(binary, mode, repeat, items, dataset_sha, out_dir):
    label = f'r{repeat}-{mode}'
    if mode == 'plain':
        extra = []
        expected_packed = 0
    elif mode == 'zstd':
        extra = [
            '--compression', '--compression-algorithm', 'zstd',
            '--compression-min-idle', '0', '--compression-max-bytes-per-tick', '2mb',
            '--compression-time-budget-ms', '20',
        ]
        expected_packed = 1700  # Both 1500 `session:` and 200 `blob:` compress with Zstd dictionaries
    elif mode == 'lz4':
        extra = [
            '--compression', '--compression-algorithm', 'lz4',
            '--compression-min-idle', '0', '--compression-max-bytes-per-tick', '2mb',
            '--compression-time-budget-ms', '20',
        ]
        expected_packed = 200  # 200 large `blob:` keys compress with LZ4; 1500 small `session:` JSONs need a dictionary and are skipped by cost-benefit
    elif mode == 'auto':
        extra = [
            '--compression', '--compression-algorithm', 'auto',
            '--compression-min-idle', '0', '--compression-max-bytes-per-tick', '2mb',
            '--compression-time-budget-ms', '20',
        ]
        expected_packed = 1700  # 1500 `session:` via Zstd dict + 200 `blob:` via LZ4; 100 random skipped
    else:
        raise ValueError(mode)

    proc, log_file, port, server_cmd = start_server(binary, extra, out_dir / f'{label}.log')
    try:
        with Client(port) as client:
            compressible_items = items[:1700]
            random_items = items[1700:]
            pipeline_set(client, compressible_items)
            if expected_packed > 0:
                deadline = time.monotonic() + 25
                while True:
                    info = parse_info(client.command('INFO').decode())
                    if int(info['compressed_keys']) == expected_packed:
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError(
                            f"{label}: timed out waiting for {expected_packed} packed keys, got {info['compressed_keys']}"
                        )
                    time.sleep(0.05)

            pipeline_set(client, random_items)
            if expected_packed > 0:
                expected_skips = (1700 - expected_packed) + len(random_items)
                deadline = time.monotonic() + 10
                while True:
                    info = parse_info(client.command('INFO').decode())
                    if int(info['compression_cost_benefit_skips']) >= expected_skips:
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError(
                            f"{label}: timed out waiting for {expected_skips} cost_benefit_skips, got {info['compression_cost_benefit_skips']}"
                        )
                    time.sleep(0.05)

            small_keys = [k for k, _ in items[:1500]]
            large_keys = [k for k, _ in items[1500:1700]]
            small_read = measure_gets(client, small_keys, rounds=4, pipeline=16)
            large_read = measure_gets(client, large_keys, rounds=20, pipeline=16)
            verify_all(client, items)

            info = parse_info(client.command('INFO').decode())
            used_mem = int(info['used_memory'])
            prep_dict = int(info['compression_prepared_dict_bytes'])
            decomp_calls = int(info['compression_decompress_calls'])
            decomp_us = int(info['compression_decompress_us'])
            record = {
                'name': label,
                'mode': mode,
                'repeat': repeat,
                'server_command': server_cmd,
                'dataset_sha256': dataset_sha,
                'keys_total': len(items),
                'used_memory_bytes': used_mem,
                'prepared_dict_bytes': prep_dict,
                'total_footprint_bytes': used_mem + prep_dict,
                'net_memory_saved_bytes': int(info['compression_net_memory_saved_bytes']),
                'compressed_keys': int(info['compressed_keys']),
                'compressed_zstd_keys': int(info['compressed_zstd_keys']),
                'compressed_lz4_keys': int(info['compressed_lz4_keys']),
                'cost_benefit_skips': int(info['compression_cost_benefit_skips']),
                'compression_ratio': float(info['compression_ratio']),
                'small_get': small_read,
                'large_get': large_read,
                'observed_p95_us': int(info['compression_observed_p95_us']),
                'observed_p99_us': int(info['compression_observed_p99_us']),
                'decompress_calls': decomp_calls,
                'decompress_us': decomp_us,
                'decompress_us_per_call': round(decomp_us / decomp_calls, 3) if decomp_calls else 0.0,
                'verified_count': len(items),
            }
            (out_dir / f'{label}.json').write_text(json.dumps(record, indent=2) + '\n')
            return record
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        log_file.close()


def run_drift(binary, repeat, out_dir):
    label = f'r{repeat}-drift'
    extra = [
        '--compression', '--compression-algorithm', 'auto',
        '--compression-min-idle', '0', '--compression-max-bytes-per-tick', '2mb',
        '--compression-time-budget-ms', '20',
    ]
    v1_pairs = [(f'order:{i}'.encode(), session_json(i)) for i in range(1100)]
    v2_pairs = [(f'order:v2_{i}'.encode(), drifted_xml(i)) for i in range(1100)]
    proc, log_file, port, server_cmd = start_server(binary, extra, out_dir / f'{label}.log')
    try:
        with Client(port) as client:
            pipeline_set(client, v1_pairs)
            deadline = time.monotonic() + 25
            while True:
                info = parse_info(client.command('INFO').decode())
                if int(info['compressed_keys']) == len(v1_pairs):
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f'{label}: v1 compaction timeout')
                time.sleep(0.05)
            pre_drift_ratio = float(info['compression_ratio'])

            t0 = time.perf_counter()
            pipeline_set(client, v2_pairs)
            deadline = time.monotonic() + 30
            while True:
                info = parse_info(client.command('INFO').decode())
                if (
                    int(info['compressed_keys']) == len(v1_pairs) + len(v2_pairs)
                    and int(info['compression_dicts_retrained']) >= 1
                    and int(info['compression_dicts_active']) == 1
                    and int(info['compression_dicts_retired']) == 1
                ):
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f'{label}: drift retraining timeout: {info}')
                time.sleep(0.05)
            retrain_ms = round((time.perf_counter() - t0) * 1000.0, 2)
            verify_all(client, v1_pairs)
            verify_all(client, v2_pairs)

            # Now update the v1 keys to the v2 schema so the retired v1 dictionary drains and is reclaimed.
            v1_upgraded = [(f'order:{i}'.encode(), drifted_xml(i + 10_000)) for i in range(1100)]
            t1 = time.perf_counter()
            pipeline_set(client, v1_upgraded)
            deadline = time.monotonic() + 25
            while True:
                info = parse_info(client.command('INFO').decode())
                if (
                    int(info['compressed_keys']) == len(v1_upgraded) + len(v2_pairs)
                    and int(info['compression_dicts_reclaimed']) >= 1
                    and int(info['compression_dicts_retired']) == 0
                ):
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f'{label}: retired dict reclamation timeout: {info}')
                time.sleep(0.05)
            migration_ms = round(retrain_ms + (time.perf_counter() - t1) * 1000.0, 2)

            verify_all(client, v1_upgraded)
            verify_all(client, v2_pairs)
            record = {
                'name': label,
                'repeat': repeat,
                'server_command': server_cmd,
                'v1_keys': len(v1_pairs),
                'v2_keys': len(v2_pairs),
                'pre_drift_ratio': pre_drift_ratio,
                'post_migration_ratio': float(info['compression_ratio']),
                'dict_drifts': int(info['compression_dict_drifts']),
                'dicts_retrained': int(info['compression_dicts_retrained']),
                'dicts_reclaimed': int(info['compression_dicts_reclaimed']),
                'dicts_active': int(info['compression_dicts_active']),
                'dicts_retired': int(info['compression_dicts_retired']),
                'net_memory_saved_bytes': int(info['compression_net_memory_saved_bytes']),
                'retrain_wall_ms': retrain_ms,
                'migration_wall_ms': migration_ms,
                'verified_count': len(v1_upgraded) + len(v2_pairs),
            }
            (out_dir / f'{label}.json').write_text(json.dumps(record, indent=2) + '\n')
            return record
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        log_file.close()


def check(directory):
    s = json.loads((directory / 'summary.json').read_text())
    assert len(s['mode_records']) == s['repeats'] * 4
    assert len(s['drift_records']) == s['repeats']
    for name, digest in s['files'].items():
        actual = hashlib.sha256((directory / name).read_bytes()).hexdigest()
        assert actual == digest, f'hash mismatch for {name}'
    for r in s['mode_records']:
        assert r['verified_count'] == 1800
        if r['mode'] == 'auto':
            assert r['compressed_zstd_keys'] == 1500
            assert r['compressed_lz4_keys'] == 200
            assert r['cost_benefit_skips'] >= 100
            assert r['net_memory_saved_bytes'] > 0
    for d in s['drift_records']:
        assert d['verified_count'] == 2200
        assert d['dict_drifts'] >= 1
        assert d['dicts_retrained'] >= 1
        assert d['dicts_reclaimed'] >= 1
        assert d['dicts_retired'] == 0
        assert d['dicts_active'] == 1
    print(
        f"Checked {len(s['mode_records'])} mode records and {len(s['drift_records'])} drift records: "
        "hashes, adaptive Zstd/LZ4/Plain routing, dynamic retraining/reclamation, and integrity verified."
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/crabcache')
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument(
        '--output',
        type=Path,
        default=ROOT / 'docs/benchmark-results/2026-10-19-crabpack-v2',
    )
    parser.add_argument('--check', type=Path)
    args = parser.parse_args()
    if args.check:
        check(args.check)
        return

    binary = args.binary.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    items, dataset_sha = build_mixed_dataset()
    raw_payload_bytes = sum(len(v) for _, v in items)

    mode_records = []
    drift_records = []
    modes = ('plain', 'zstd', 'lz4', 'auto')
    for repeat in range(1, args.repeats + 1):
        for mode in modes:
            rec = run_mode(binary, mode, repeat, items, dataset_sha, args.output)
            mode_records.append(rec)
            print(json.dumps(rec), flush=True)
        drec = run_drift(binary, repeat, args.output)
        drift_records.append(drec)
        print(json.dumps(drec), flush=True)

    medians = {}
    for mode in modes:
        group = [r for r in mode_records if r['mode'] == mode]
        medians[mode] = {
            'used_memory_bytes': int(statistics.median(r['used_memory_bytes'] for r in group)),
            'prepared_dict_bytes': int(statistics.median(r['prepared_dict_bytes'] for r in group)),
            'total_footprint_bytes': int(statistics.median(r['total_footprint_bytes'] for r in group)),
            'net_memory_saved_bytes': int(statistics.median(r['net_memory_saved_bytes'] for r in group)),
            'compressed_zstd_keys': int(statistics.median(r['compressed_zstd_keys'] for r in group)),
            'compressed_lz4_keys': int(statistics.median(r['compressed_lz4_keys'] for r in group)),
            'cost_benefit_skips': int(statistics.median(r['cost_benefit_skips'] for r in group)),
            'compression_ratio': round(statistics.median(r['compression_ratio'] for r in group), 2),
            'small_get_ops_per_sec': round(statistics.median(r['small_get']['ops_per_sec'] for r in group), 1),
            'small_get_p99_us': round(statistics.median(r['small_get']['p99_us'] for r in group), 2),
            'large_get_ops_per_sec': round(statistics.median(r['large_get']['ops_per_sec'] for r in group), 1),
            'large_get_p99_us': round(statistics.median(r['large_get']['p99_us'] for r in group), 2),
            'decompress_us_per_call': round(statistics.median(r['decompress_us_per_call'] for r in group), 3),
        }

    drift_median = {
        'pre_drift_ratio': round(statistics.median(r['pre_drift_ratio'] for r in drift_records), 2),
        'post_migration_ratio': round(statistics.median(r['post_migration_ratio'] for r in drift_records), 2),
        'migration_wall_ms': round(statistics.median(r['migration_wall_ms'] for r in drift_records), 2),
        'dicts_retrained': int(statistics.median(r['dicts_retrained'] for r in drift_records)),
        'dicts_reclaimed': int(statistics.median(r['dicts_reclaimed'] for r in drift_records)),
        'net_memory_saved_bytes': int(statistics.median(r['net_memory_saved_bytes'] for r in drift_records)),
    }

    files = {
        p.name: hashlib.sha256(p.read_bytes()).hexdigest()
        for p in sorted(args.output.iterdir())
        if p.name != 'summary.json' and p.suffix in ('.json', '.log')
    }
    summary = {
        'purpose': 'CrabPack v2 validation: adaptive compression (Zstd/LZ4/Plain), dynamic dictionary retraining & reclamation, and cost-benefit accounting',
        'platform': platform.platform(),
        'repeats': args.repeats,
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'git_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'dataset_sha256': dataset_sha,
        'raw_payload_bytes': raw_payload_bytes,
        'medians': medians,
        'drift_median': drift_median,
        'mode_records': mode_records,
        'drift_records': drift_records,
        'files': files,
    }
    (args.output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    check(args.output)


if __name__ == '__main__':
    main()
