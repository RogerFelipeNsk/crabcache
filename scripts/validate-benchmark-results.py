#!/usr/bin/env python3
"""Recalculate published summaries from raw measurements; no server is started."""
import json
import math
from pathlib import Path
import re
from statistics import median
import sys
import zipfile

from benchmark import parse_memtier, parse_redis_benchmark, require, validate_get_counts


def load_outputs(folder):
    files = {p.name: p.read_text() for p in folder.glob('*-load.stdout')}
    archive = folder / 'traces.zip'
    if archive.exists():
        with zipfile.ZipFile(archive) as traces:
            for name in traces.namelist():
                if name.endswith('-load.stdout'):
                    require(name not in files, 'duplicate load evidence')
                    files[name] = traces.read(name).decode()
    return [files[name] for name in sorted(files)]

root = Path(sys.argv[1])
checked = 0
binary_hash = None
for name, expected in [('core', 12), ('memory', 18), ('compression', 45), ('default', 24)]:
    folder = root / name
    summary = json.loads((folder / 'summary.json').read_text())
    results = summary['results']
    meta = summary['metadata']
    require(meta['repeats'] == 3 and meta['seconds'] == 10, 'unexpected run settings')
    binary_hash = binary_hash or meta['binary_sha256']
    require(meta['binary_sha256'] == binary_hash, 'different server binaries')
    require(len(results) == expected, f'{name}: expected {expected} rows, got {len(results)}')
    groups = {}
    for row in results:
        key = tuple(row.get(k) for k in ('mode', 'server', 'dataset', 'value_size', 'clients', 'pipeline'))
        groups.setdefault(key, []).append(row['repeat'])
    require(all(sorted(repeats) == [1, 2, 3] for repeats in groups.values()), 'missing or duplicate repeat')
    csv_outputs = sorted(folder.glob('*-redis-benchmark.stdout'))
    loads = load_outputs(folder)
    require(len(loads) == len(results), 'missing raw load output')
    for index, row in enumerate(results):
        load = re.search(r'errors:\s*(\d+), replies:\s*(\d+)', loads[index])
        require(load is not None and int(load[1]) == 0 and int(load[2]) == row['keys'], 'invalid raw load')
        datasets = [json.loads(p.read_text()) for p in folder.glob('*.dataset.json')]
        require(any(all(d[k] == row[k] for k in ('keys', 'value_bytes', 'mean_value_bytes', 'sha256'))
                    for d in datasets), 'dataset metadata mismatch')
        if row['mode'] == 'memory':
            require(median(row['final_samples_bytes']) == row['final_bytes'], 'memory median mismatch')
            observed_max = (max(row['post_load_samples_bytes'] + row['final_samples_bytes'])
                            - row['baseline_bytes']) / row['keys']
            require(math.isclose(observed_max, row['observed_post_load_max_bytes_per_key']), 'observed max mismatch')
            actual = (row['final_bytes'] - row['baseline_bytes']) / row['keys']
            require(math.isclose(actual, row['bytes_per_key']), 'B/key calculation mismatch')
            require(row['verified_values'] == row['keys'], 'incomplete value verification')
            if row['server'] == 'crabpack':
                require(row['compressed_keys'] == row['keys'], 'incomplete packing')
                require(row['compressed_original_bytes'] == row['value_bytes'], 'packed original size mismatch')
                ratio = row['compressed_original_bytes'] / row['compressed_stored_bytes']
                require(abs(ratio - row['compression_ratio']) <= .0051, 'compression ratio mismatch')
        elif name == 'default':
            parsed = parse_redis_benchmark(csv_outputs[index].read_text())
            for command in ('SET', 'GET'):
                require(parsed[command] == row[command], 'CSV/summary mismatch')
        else:
            path = folder / f"{index:03d}-{row['server']}-p{row['pipeline']}.memtier.json"
            raw = json.loads(path.read_text())
            parsed = parse_memtier(raw)
            require(all(parsed[key] == row[key] for key in parsed), 'memtier/summary mismatch')
            count = raw['ALL STATS']['Totals']['Count']
            require(count == row['ops_count'], 'operation count mismatch')
            require(math.isclose(count / row['cpu_seconds'], row['ops_per_cpu_second']), 'ops/CPU mismatch')
            require(math.isclose(row['cpu_seconds'] / row['wall_seconds'], row['mean_cpu_cores']), 'CPU mismatch')
            if name == 'compression':
                require(row['verified_values'] == row['keys'], 'incomplete GET verification')
                unreported = validate_get_counts(row['server_get_hits'], row['client_get_count'],
                                                 row['clients'], row['pipeline'])
                require(unreported == row['unreported_gets'], 'in-flight count mismatch')
                if row['server'] == 'crabpack':
                    require(row['compressed_keys_after'] == row['keys'], 'packed coverage changed')
        require(math.isclose(row['value_bytes'] / row['keys'], row['mean_value_bytes']), 'value size mismatch')
        checked += 1
print(f'{checked} measurements independently recalculated and matched raw output')
