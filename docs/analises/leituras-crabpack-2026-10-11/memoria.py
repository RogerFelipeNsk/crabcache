#!/usr/bin/env python3
"""Paired memory diagnostic; emits raw JSON and verifies all session values.

KEYS=10000 OUT_DIR=<new directory> python3 memoria.py
This is one diagnostic round, not a statistical performance benchmark.
"""
import argparse
import contextlib
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'scripts'))
from benchmark import Bench, physical_bytes, require


def main():
    count = int(os.getenv('KEYS', '10000'))
    require(count >= 2000, 'need at least 2000 session keys')
    subprocess.run(['cargo', 'build', '--release', '--locked', '--bin', 'crabcache', '--example', 'dataset'],
                   cwd=ROOT, check=True)
    os.environ['REPEATS'] = '1'
    os.environ['SECS'] = '4'
    bench = Bench(argparse.Namespace(mode='compression', keys=count, ports=[]))
    source, dataset, samples = bench.dataset('session', count)
    bench.metadata.update(analysis='Paired memory timeline', keys=count,
                          dataset=dataset, rounds=1,
                          note='Two fresh owned servers; no user GET workload during memory samples.')
    bench.save()
    with contextlib.ExitStack() as stack:
        servers = {}
        for name in ('crabcache', 'crabpack'):
            client, pid, port = stack.enter_context(bench.server(name, threads=1, packed=name == 'crabpack'))
            baseline = physical_bytes(pid)
            bench.load(client, port, source, count, samples)
            servers[name] = (client, pid, baseline)
        client, pid, _ = servers['crabpack']
        bench.pack(client, count, pid)
        start = time.monotonic()
        for wanted in (.5, 2, 5, 12, 20):
            delay = wanted - (time.monotonic() - start)
            if delay > 0:
                time.sleep(delay)
            for name, (client, pid, baseline) in servers.items():
                footprint = physical_bytes(pid)
                info = bench.info(client)
                require(client.command('DBSIZE') == count, 'keyspace changed')
                bench.record({'mode': 'memory-timeline', 'server': name,
                              'elapsed_seconds': time.monotonic() - start,
                              'sample_after_seconds': wanted, 'baseline_bytes': baseline,
                              'physical_bytes': footprint,
                              'incremental_bytes_per_key': (footprint - baseline) / count,
                              'used_memory_bytes_per_key': int(info['used_memory']) / count,
                              'compressed_keys': int(info['compressed_keys'])})
        for client, _, _ in servers.values():
            bench.verify_all(client, source)
        for row in bench.results:
            row['verified_values_after_timeline'] = count
        bench.save()
    print('Memory timeline and full readbacks completed:', bench.out)


if __name__ == '__main__':
    main()
