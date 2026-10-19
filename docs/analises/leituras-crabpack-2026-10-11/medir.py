#!/usr/bin/env python3
"""Supplementary, reproducible GET analysis; does not modify published benchmarks.

Run from the repository with Python 3 and the native benchmark tools installed.
SECS=4 REPEATS=3 KEYS=10000 OUT_DIR=<new directory> python3 <this file>
Only fresh, owned loopback servers are used. Compares storage compression,
not encryption, TLS or persistence restoration.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'scripts'))
from benchmark import (Bench, cpu_seconds, dataset_info, encode, physical_bytes,
                       read_resp, require, validate_get_counts, parse_memtier)


def values_from(path):
    values = []
    with path.open('rb') as stream:
        while stream.peek(1):
            command = read_resp(stream)
            require(len(command) == 3 and command[0] == b'SET', 'invalid input SET')
            values.append(command[2])
    return values


def large_api(i):
    # A structured API page with 40 distinct records, not repeated padding bytes.
    return json.dumps({
        'request_id': hashlib.sha256(f'request:{i}'.encode()).hexdigest()[:32],
        'status': 200,
        'data': {'items': [{
            'id': i * 40 + j,
            'status': ('ok', 'pending', 'failed')[(i + j) % 3],
            'amount': round(((i * 13 + j * 7) % 100000) / 100, 2),
            'at': f'2026-10-{1 + (i + j) % 28:02d}T12:30:00Z',
            'category': ('casa', 'moda', 'eletronicos')[(i + j) % 3],
        } for j in range(40)], 'page': 1 + i % 100, 'has_more': bool(i % 2)},
        'took_ms': 1 + i % 900,
    }, separators=(',', ':')).encode()


def make_datasets(bench, n):
    bases = {}
    for kind in ('session', 'product', 'api'):
        path = bench.out / f'{kind}-source.resp'
        with path.open('wb') as output:
            subprocess.run([str(ROOT / 'target/release/examples/dataset'), kind, str(n)],
                           stdout=output, check=True)
        bases[kind] = values_from(path)
    bases['api_large'] = [large_api(i) for i in range(n)]
    labels = {'session': 'Sessão JSON', 'product': 'Produto JSON', 'api': 'API JSON',
              'api_large': 'API JSON · 40 itens'}
    datasets = []
    for name in labels:
        values = bases[name]
        prefix = name + ':'
        path = bench.out / f'{name}.resp'
        with path.open('wb') as output:
            for i, value in enumerate(values):
                output.write(encode(['SET', f'{prefix}{i}', value]))
        metadata, samples = dataset_info(path, n)
        metadata.update(dataset=name, label=labels[name], encrypted=False,
                        plain_mean_bytes=sum(map(len, values)) / n,
                        minimum_value_bytes=min(map(len, values)),
                        maximum_value_bytes=max(map(len, values)),
                        prefix=prefix, example_plaintext=json.loads(values[0]))
        (bench.out / f'{name}.dataset.json').write_text(json.dumps(metadata, indent=2))
        datasets.append({'name': name, 'path': path, 'metadata': metadata,
                         'samples': samples, 'values': values})
    return datasets


def compression_ready(client, dataset):
    deadline = time.monotonic() + 90
    count = dataset['metadata']['keys']
    while True:
        info = Bench.info(client)
        if int(info['compressed_keys']) == count:
            return info
        require(time.monotonic() < deadline,
                f'compression not ready for {dataset["name"]}: {info}')
        time.sleep(.2)


def application_readback(client, dataset, samples=2500):
    # One persistent connection, sequential GETs, including UTF-8/JSON decoding.
    ids = [random.Random(4000 + i).randrange(dataset['metadata']['keys'])
           for i in range(samples)]
    gets_ns, processing_ns, totals_ns = [], [], []
    for index in ids[:100]:
        value = client.command('GET', f'{dataset["metadata"]["prefix"]}{index}')
        json.loads(value)
    for index in ids:
        start = time.perf_counter_ns()
        value = client.command('GET', f'{dataset["metadata"]["prefix"]}{index}')
        received = time.perf_counter_ns()
        require(value == dataset['values'][index], 'application GET mismatch')
        require(value == dataset['values'][index], 'plaintext mismatch')
        parsed = json.loads(value)
        finished = time.perf_counter_ns()
        require(isinstance(parsed, dict), 'expected a JSON object')
        gets_ns.append(received - start)
        processing_ns.append(finished - received)
        totals_ns.append(finished - start)
    return {'samples': samples, 'get_mean_us': statistics.mean(gets_ns) / 1000,
            'processing_mean_us': statistics.mean(processing_ns) / 1000,
            'ready_object_mean_us': statistics.mean(totals_ns) / 1000,
            'ready_object_p50_us': statistics.median(totals_ns) / 1000,
            'ready_object_p99_us': sorted(totals_ns)[int(.99 * (samples - 1))] / 1000,
            'note': 'Python client; processing includes correctness checks and JSON parsing'}


def measure_get(bench, client, pid, port, dataset, config, repeat, server):
    threads, clients, pipeline = config['threads'], config['clients'], config['pipeline']
    connections = threads * clients
    raw = bench.out / f'{dataset["name"]}-{server}-r{repeat}-{config["id"]}.memtier.json'
    command = ['memtier_benchmark', '-s', '127.0.0.1', '-p', str(port), '--protocol=redis',
               '-t', str(threads), '-c', str(clients), f'--pipeline={pipeline}', '--ratio=0:1',
               '-d', '100', f'--key-prefix={dataset["metadata"]["prefix"]}', '--key-minimum=1',
               f'--key-maximum={dataset["metadata"]["keys"] - 1}', '--key-pattern=R:R',
               '--distinct-client-seed', f'--test-time={bench.secs}', '--hide-histogram',
               '--print-percentiles=50,99', f'--json-out-file={raw}']
    for _ in range(5):
        bench.verify(client, dataset['samples'])
    before = bench.info(client)
    cpu0, wall0 = cpu_seconds(pid), time.monotonic()
    bench.run(command, f'{dataset["name"]}-{server}-{config["id"]}', timeout=bench.secs + 60)
    elapsed, cpu = time.monotonic() - wall0, cpu_seconds(pid) - cpu0
    after = bench.info(client)
    require(int(after['keyspace_misses']) == int(before['keyspace_misses']), 'server GET misses')
    require(int(after['evicted_keys']) == int(before['evicted_keys']), 'unexpected eviction')
    data = json.loads(raw.read_text())
    result = parse_memtier(data)
    totals = data['ALL STATS']['Totals']
    gets = data['ALL STATS']['Gets']
    hits = int(after['keyspace_hits']) - int(before['keyspace_hits'])
    unreported = validate_get_counts(hits, gets['Count'], connections, pipeline)
    require(cpu > 0 and totals['Count'] == gets['Count'], 'invalid CPU timer or non-GET workload')
    mean_ms = totals['Accumulated Latency'] / totals['Count']
    require(abs(mean_ms - totals['Average Latency']) < .0011, 'mean latency disagrees with raw data')
    result.update(mode='get-analysis', server=server, dataset=dataset['name'], repeat=repeat,
                  config=config['id'], connections=connections, pipeline=pipeline,
                  mean_latency_ms=mean_ms, accumulated_latency_ms=totals['Accumulated Latency'],
                  cpu_seconds=cpu, wall_seconds=elapsed, mean_cpu_cores=cpu / elapsed,
                  cpu_us_per_get=cpu * 1e6 / totals['Count'], client_get_count=gets['Count'],
                  server_get_hits=hits, unreported_gets=unreported,
                  max_unreported_gets=connections * pipeline, raw_json=raw.name,
                  mean_value_bytes=dataset['metadata']['mean_value_bytes'],
                  encrypted=dataset['metadata']['encrypted'],
                  compressed_keys=int(after['compressed_keys']),
                  compression_dicts_rejected=int(after['compression_dicts_rejected']))
    bench.verify_all(client, dataset['path'])
    require(client.command('DBSIZE') == dataset['metadata']['keys'], 'keyspace changed')
    require(int(bench.info(client)['compressed_keys']) == result['compressed_keys'],
            'packed coverage changed during readback')
    result['verified_values'] = dataset['metadata']['keys']
    return result


def main():
    args = argparse.Namespace(mode='compression', keys=int(os.getenv('KEYS', '10000')), ports=[])
    require(args.keys >= 2000, 'need at least 2000 keys for this analysis')
    subprocess.run(['cargo', 'build', '--release', '--locked', '--bin', 'crabcache', '--example', 'dataset'],
                   cwd=ROOT, check=True)
    bench = Bench(args)
    bench.metadata.update(analysis='GET latency, server CPU and JSON object decoding',
                          measurement_date=time.strftime('%Y-%m-%d'),
                          python=sys.version,
                          native=True, tls=False, keys_per_dataset=args.keys,
                          configs=[{'id': 'single', 'threads': 1, 'clients': 1, 'pipeline': 1},
                                   {'id': 'pipeline16', 'threads': 4, 'clients': 12, 'pipeline': 16}],
                          notes=['Application source unchanged; fresh owned loopback servers.',
                                 'Storage compression is transparent; JSON decoding happens in the Python client.',
                                 'CPU includes all server threads and enabled compression background work.',
                                 'Short local repetitions on a shared machine; not production guarantees.'])
    datasets = make_datasets(bench, args.keys)
    bench.metadata['datasets'] = [d['metadata'] for d in datasets]
    bench.save()
    configurations = bench.metadata['configs']
    for dataset in datasets:
        for repeat in range(1, bench.repeats + 1):
            targets = ['crabcache', 'crabpack'] if repeat % 2 else ['crabpack', 'crabcache']
            for name in targets:
                with bench.server(name, threads=1, packed=name == 'crabpack') as (client, pid, port):
                    baseline = statistics.median(physical_bytes(pid) for _ in range(3))
                    bench.load(client, port, dataset['path'], args.keys, dataset['samples'])
                    load_stdout = f'{bench.serial:03d}-load.stdout'
                    if name == 'crabpack':
                        compression_ready(client, dataset)
                    time.sleep(.5)
                    final = statistics.median(physical_bytes(pid) for _ in range(3))
                    memory = bench.info(client)
                    bench.verify_all(client, dataset['path'])
                    bench.record({'mode': 'memory-analysis', 'server': name,
                                  'dataset': dataset['name'], 'repeat': repeat,
                                  'load_stdout': load_stdout,
                                  'keys': args.keys, 'verified_values': args.keys,
                                  'baseline_bytes': baseline, 'final_bytes': final,
                                  'bytes_per_key': (final - baseline) / args.keys,
                                  'used_memory_bytes': int(memory['used_memory']),
                                  'compressed_keys': int(memory['compressed_keys']),
                                  'compressed_original_bytes': int(memory['compressed_original_bytes']),
                                  'compressed_stored_bytes': int(memory['compressed_stored_bytes']),
                                  'compression_dicts_rejected': int(memory['compression_dicts_rejected'])})
                    configs = configurations if repeat % 2 else configurations[::-1]
                    for config in configs:
                        result = measure_get(bench, client, pid, port, dataset, config, repeat, name)
                        result['load_stdout'] = load_stdout
                        bench.record(result)
                    application = application_readback(client, dataset)
                    bench.record({'mode': 'application-analysis', 'server': name,
                                  'dataset': dataset['name'], 'repeat': repeat,
                                  'load_stdout': load_stdout, **application})
    print('All supplemental scenarios completed; raw input and measurements:', bench.out, flush=True)


if __name__ == '__main__':
    main()
