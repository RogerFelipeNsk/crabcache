#!/usr/bin/env python3
"""Paired footprint validation with the playground dataset; loopback, not Fly performance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import socket
import subprocess
import time

from benchmark import Client, Bench

ROOT = Path(__file__).resolve().parents[1]


def display_path(path):
    path = Path(path).resolve()
    for base, prefix in ((ROOT.parent, '<workspace>'), (Path.home(), '<home>')):
        if path.is_relative_to(base):
            rel = path.relative_to(base).as_posix()
            return prefix if rel == '.' else f'{prefix}/{rel}'
    return path.as_posix()


def run(binary, api, label, out):
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    cmd = [str(binary), '--bind', '127.0.0.1', '--port', str(port), '--threads', '1',
           '--shards', '64', '--compression', '--compression-min-idle', '0',
           '--compression-min-size', '40', '--maxmemory-policy', 'allkeys-lfu']
    env = {k: v for k, v in os.environ.items() if not k.startswith('CRABCACHE_')}
    with (out / (label + '.log')).open('w') as log:
        proc = subprocess.Popen(cmd, env=env, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 10
            while True:
                try:
                    with Client(port) as client:
                        baseline = Bench.info(client)
                    break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise
                    time.sleep(.02)
            env.update(CACHE_ADDR=f'127.0.0.1:{port}', CACHE_ENABLED='true', CACHE_PASSWORD='')
            invocation = [str(api), 'cache-bench', '1500', '60']
            result = subprocess.run(invocation, env=env, capture_output=True, text=True,
                                    timeout=90, check=True)
            parsed = json.loads(result.stdout)
            (out / (label + '.json')).write_text(result.stdout)
            assert parsed['integrity_check']['passed']
            assert parsed['integrity_check']['verified_count'] == parsed['keys_total'] == 4500
            assert parsed['compaction_status'] == 'complete'
            assert parsed['compression_stats']['keys_compressed'] == 4500
            info = parsed['info_final']
            entry_bytes = int(info['used_memory'])
            prepared = int(info['compression_prepared_dict_bytes'])
            return {'name': label, 'server_command': [display_path(binary), *cmd[1:]],
                    'client_command': [display_path(api), *invocation[1:]],
                    'file': label + '.json', 'baseline_info': baseline,
                    'dataset_sha256': parsed['dataset_sha256'],
                    'raw_payload_bytes': parsed['raw_payload_bytes_total'],
                    'stored_payload_bytes': parsed['compression_stats']['stored_bytes'],
                    'entry_bytes': entry_bytes, 'prepared_dict_bytes': prepared,
                    'known_components_bytes': entry_bytes + prepared,
                    'training_wall_us': int(info['compression_training_us']),
                    'packing_wall_us': int(info['compression_pack_us']),
                    'verified_count': parsed['integrity_check']['verified_count']}
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


def check(directory):
    s = json.loads((directory / 'summary.json').read_text())
    assert len(s['records']) == s['repeats'] * 2
    assert {r['name'] for r in s['records']} == {
        f'r{i}-{name}' for i in range(1, s['repeats'] + 1) for name in ('baseline', 'candidate')}
    for name, digest in s['files'].items():
        assert hashlib.sha256((directory / name).read_bytes()).hexdigest() == digest, name
    for r in s['records']:
        raw = json.loads((directory / r['file']).read_text())
        info = raw['info_final']
        assert r['entry_bytes'] == int(info['used_memory']) == raw['memory_final_bytes']
        assert r['prepared_dict_bytes'] == int(info['compression_prepared_dict_bytes'])
        assert r['known_components_bytes'] == r['entry_bytes'] + r['prepared_dict_bytes']
        assert r['stored_payload_bytes'] == raw['compression_stats']['stored_bytes']
        assert r['raw_payload_bytes'] == raw['raw_payload_bytes_total']
        assert raw['compaction_status'] == 'complete'
        assert raw['integrity_check']['passed'] and raw['integrity_check']['verified_count'] == 4500
        assert r['dataset_sha256'] == raw['dataset_sha256'] == s['dataset_sha256']
    print(f"Checked {len(s['records'])} footprint records, hashes, full coverage and integrity.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path)
    parser.add_argument('--candidate', type=Path, default=ROOT / 'target/release/crabcache')
    parser.add_argument('--api', type=Path)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--output', type=Path, default=ROOT / 'docs/benchmark-results' /
                        time.strftime('%Y-%m-%d-crabpack-adaptive-footprint-%H%M%S'))
    parser.add_argument('--check', type=Path)
    args = parser.parse_args()
    if args.check:
        check(args.check)
        return
    if not args.baseline or not args.api or args.repeats < 1:
        parser.error('--baseline and --api are required; repeats must be positive')
    binaries = {'baseline': args.baseline.resolve(), 'candidate': args.candidate.resolve()}
    api = args.api.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    summary = {'purpose': 'local footprint validation on macOS, not Fly latency/RSS or a CPU benchmark',
               'platform': platform.platform(), 'repeats': args.repeats, 'records': [],
               'binaries_sha256': {k: hashlib.sha256(p.read_bytes()).hexdigest() for k, p in binaries.items()},
               'api_sha256': hashlib.sha256(api.read_bytes()).hexdigest(),
               'git_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()}
    for repeat in range(1, args.repeats + 1):
        order = ('baseline', 'candidate') if repeat % 2 else ('candidate', 'baseline')
        for name in order:
            record = run(binaries[name], api, f'r{repeat}-{name}', args.output)
            summary['records'].append(record)
            print(json.dumps(record), flush=True)
    hashes = {r['dataset_sha256'] for r in summary['records']}
    assert len(hashes) == 1
    summary['dataset_sha256'] = hashes.pop()
    summary['files'] = {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                        for p in args.output.iterdir() if p.suffix in ('.json', '.log')}
    (args.output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    check(args.output)


if __name__ == '__main__':
    main()
