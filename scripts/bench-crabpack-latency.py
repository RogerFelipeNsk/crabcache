#!/usr/bin/env python3
"""Paired local CrabPack measurements; isolated processes, cold transition and fully packed GET."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import socket
import subprocess
import threading
import time
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def display_path(path):
    path = Path(path).resolve()
    for base, prefix in ((ROOT.parent, '<workspace>'), (Path.home(), '<home>')):
        if path.is_relative_to(base):
            rel = path.relative_to(base).as_posix()
            return prefix if rel == '.' else f'{prefix}/{rel}'
    return path.as_posix()


def encode(*args):
    args = [a if isinstance(a, bytes) else str(a).encode() for a in args]
    return b'*%d\r\n' % len(args) + b''.join(b'$%d\r\n' % len(a) + a + b'\r\n' for a in args)


class Client:
    def __init__(self, port):
        self.sock = socket.create_connection(('127.0.0.1', port), timeout=20)
        self.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.reader = self.sock.makefile('rb')

    def read(self):
        line = self.reader.readline()
        if not line.endswith(b'\r\n'):
            raise RuntimeError('incomplete RESP header')
        kind, payload = line[:1], line[1:-2]
        if kind == b'-':
            raise RuntimeError(payload.decode(errors='replace'))
        if kind == b'+':
            return payload
        if kind == b':':
            return int(payload)
        if kind == b'$':
            n = int(payload)
            if n < 0:
                return None
            value = self.reader.read(n)
            if len(value) != n or self.reader.read(2) != b'\r\n':
                raise RuntimeError('incomplete RESP bulk')
            return value
        if kind == b'*':
            return [self.read() for _ in range(int(payload))]
        raise RuntimeError(f'unexpected RESP {kind!r}')

    def command(self, *args):
        self.sock.sendall(encode(*args))
        return self.read()

    def info(self):
        raw = self.command('INFO').decode()
        return dict(l.split(':', 1) for l in raw.splitlines() if ':' in l and not l.startswith('#'))

    def close(self):
        self.reader.close()
        self.sock.close()


def measure_process(pid):
    parts = subprocess.check_output(['ps', '-p', str(pid), '-o', 'time=', '-o', 'rss='], text=True).split()
    cpu = sum(float(p) * 60**i for i, p in enumerate(reversed(parts[0].split(':'))))
    return {'cpu_seconds': cpu, 'rss_bytes': int(parts[1]) * 1024}


def dataset(kind):
    n = 10000 if kind == 'small-json' else 128
    items = []
    for i in range(1, n + 1):
        obj = {'uid': i, 'name': f'Cliente {i}', 'email': f'cliente{i}@example.test',
               'locale': 'pt-BR', 'roles': ['customer', 'reader'], 'csrf': hashlib.sha256(str(i).encode()).hexdigest(),
               'created_at': '2026-10-01T12:00:00Z', 'theme': 'dark', 'balance_cents': 100000+i}
        if kind == 'large-json':
            obj['transactions'] = [{'id': i*10000+j, 'type': 'card_purchase', 'amount_cents': j*37,
                                    'merchant': f'Loja {j%37}', 'description': 'Compra no cartao',
                                    'created_at': '2026-10-01T12:00:00Z'} for j in range(900)]
        value = json.dumps(obj, sort_keys=True, separators=(',', ':')).encode()
        items.append((f'session:{i}'.encode(), value))
    return items


def verify(client, items):
    for start in range(0, len(items), 128):
        batch = items[start:start+128]
        client.sock.sendall(b''.join(encode('GET', k) for k, _ in batch))
        for key, expected in batch:
            if client.read() != expected:
                raise AssertionError(f'value mismatch for {key!r}')


def parsed_gets(raw):
    stats = raw['ALL STATS']
    gets = stats['Gets']
    if stats['Sets']['Count'] != 0 or gets['Count'] <= 0 or gets['Misses/sec'] != 0:
        raise AssertionError('invalid GET-only measurement or cache misses')
    for row in ('Gets', 'Totals'):
        for key, value in stats[row].items():
            if ('Errors' in key or key in ('Failed Commands', 'Failed Commands/sec')) and value != 0:
                raise AssertionError(f'memtier {row} {key}={value}')
    return gets


def run_one(binary, name, kind, repeat, items, seconds, out):
    label = f'r{repeat}-{kind}-{name}'
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    command = [str(binary), '--bind', '127.0.0.1', '--port', str(port), '--threads', '4',
               '--shards', '1', '--io-conns-per-thread', '0', '--maxmemory-policy', 'allkeys-lfu',
               '--compression-min-idle', '0']
    env = {k: v for k, v in os.environ.items() if not k.startswith('CRABCACHE_')}
    log = (out / f'{label}.server.log').open('w')
    process = subprocess.Popen(command, env=env, stdout=log, stderr=log)
    client = None
    stop = threading.Event()
    monitor = None
    timeline = []
    try:
        for _ in range(100):
            try:
                client = Client(port)
                break
            except OSError:
                time.sleep(.02)
        if client is None:
            raise RuntimeError('server did not start')
        baseline = {'info': client.info(), **measure_process(process.pid)}
        training = []
        if kind == 'large-json':
            # The old server starts training after 256 KiB but requires >=10 samples.
            # Train both dictionaries on the same small objects before loading large
            # values; otherwise the old server rejects its two-sample batch.
            training = [(b'session:train:'+str(i).encode(), v) for i, (_, v) in enumerate(dataset('small-json')[:2000], 1)]
            for start in range(0, len(training), 128):
                batch = training[start:start+128]
                client.sock.sendall(b''.join(encode('SET', k, v) for k, v in batch))
                if any(client.read() != b'OK' for _ in batch):
                    raise AssertionError('failed dictionary preparation load')
            client.command('CONFIG','SET','compression','yes')
            deadline = time.monotonic()+60
            while int(client.info().get('compressed_keys', 0)) != len(training):
                if time.monotonic() >= deadline:
                    raise AssertionError('dictionary preparation failed')
                time.sleep(.2)
            verify(client, training)
            client.command('CONFIG','SET','compression','no')
        total_keys = len(items)+len(training)
        for start in range(0, len(items), 128):
            batch = items[start:start+128]
            client.sock.sendall(b''.join(encode('SET', k, v) for k, v in batch))
            if any(client.read() != b'OK' for _ in batch):
                raise AssertionError('failed load')
        if client.command('DBSIZE') != total_keys:
            raise AssertionError('key count mismatch')
        started = time.monotonic()
        client.command('CONFIG', 'SET', 'compression', 'yes')

        def sample():
            observer = Client(port)
            try:
                while not stop.is_set():
                    timeline.append({'elapsed_s': time.monotonic()-started, 'info': observer.info(), **measure_process(process.pid)})
                    stop.wait(.2)
            finally:
                observer.close()
        monitor = threading.Thread(target=sample)
        monitor.start()
        records = []
        for phase in ('transition', 'packed'):
            if phase == 'packed':
                deadline = time.monotonic()+60
                while int(client.info().get('compressed_keys', 0)) != total_keys:
                    if time.monotonic() >= deadline:
                        raise AssertionError(f'{label}: incomplete compaction')
                    time.sleep(.2)
                verify(client, items)
            before = {'info': client.info(), **measure_process(process.pid)}
            raw_path = out / f'{label}-{phase}.memtier.json'
            cmd = ['memtier_benchmark', '-s', '127.0.0.1', '-p', str(port), '-t', '4', '-c', '12',
                   '--pipeline=1', '--ratio=0:1', '--key-prefix=session:', '--key-minimum=1',
                   f'--key-maximum={len(items)}', '--key-pattern=R:R', f'--test-time={seconds}',
                   '--hide-histogram', '--print-percentiles=50,95,99', f'--json-out-file={raw_path}']
            recorded_cmd = [*cmd[:-1], f'--json-out-file={display_path(raw_path)}']
            log_path = out / f'{label}-{phase}.memtier.log'
            cpu0, wall0 = before['cpu_seconds'], time.monotonic()
            with log_path.open('w') as stream:
                subprocess.run(cmd, stdout=stream, stderr=stream, check=True, timeout=seconds+30)
            wall = time.monotonic()-wall0
            log_path.write_text(log_path.read_text().replace(str(raw_path), display_path(raw_path)))
            after_process = measure_process(process.pid)
            cpu = after_process['cpu_seconds']-cpu0
            raw = json.loads(raw_path.read_text())
            gets = parsed_gets(raw)
            after = {'info': client.info(), **after_process}
            verify(client, items)
            record = {'name': name, 'dataset': kind, 'repeat': repeat, 'phase': phase,
                      'count': gets['Count'], 'ops_per_second': gets['Ops/sec'],
                      'mean_ms': gets['Accumulated Latency']/gets['Count'],
                      'p50_ms': gets['Percentile Latencies']['p50.00'],
                      'p95_ms': gets['Percentile Latencies']['p95.00'],
                      'p99_ms': gets['Percentile Latencies']['p99.00'], 'max_ms': gets['Max Latency'],
                      'cpu_seconds': cpu, 'cpu_us_per_get': cpu/gets['Count']*1e6, 'wall_seconds': wall,
                      'before': before, 'after': after, 'baseline': baseline,
                      'verified_count': len(items), 'total_keys': total_keys, 'training_keys': len(training), 'memtier_file': raw_path.name, 'command': recorded_cmd,
                      'server_command': [display_path(binary), *command[1:]], 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
            records.append(record)
            print(f'{label} {phase}: mean={record["mean_ms"]:.3f}ms p99={record["p99_ms"]:.3f}ms CPU/GET={record["cpu_us_per_get"]:.3f}us',flush=True)
        return records
    finally:
        stop.set()
        if monitor is not None:
            monitor.join(timeout=25)
        (out / f'{label}.timeline.json').write_text(json.dumps(timeline, indent=2)+'\n')
        if client is not None:
            client.close()
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        log.close()


def check(summary, directory):
    records = summary['records']
    expected = summary['repeats'] * 2 * 2 * 2
    if len(records) != expected:
        raise AssertionError(f'expected {expected} complete records, got {len(records)}')
    slots = {(r['name'],r['dataset'],r['repeat'],r['phase']) for r in records}
    expected_slots = {(name,kind,repeat,phase) for name in ('baseline','candidate')
                      for kind in ('small-json','large-json') for repeat in range(1,summary['repeats']+1)
                      for phase in ('transition','packed')}
    if slots != expected_slots:
        raise AssertionError('duplicate or missing scenario records')
    with zipfile.ZipFile(directory/'datasets.zip') as archive:
        if hashlib.sha256(archive.read('large-json-training.resp')).hexdigest() != summary['large_training_sha256']:
            raise AssertionError('training dataset hash mismatch')
        for name, meta in summary['datasets'].items():
            raw = archive.read(name+'.resp')
            if hashlib.sha256(raw).hexdigest() != meta['sha256']:
                raise AssertionError('dataset hash mismatch')
    for r in records:
        raw = json.loads((directory/r['memtier_file']).read_text())
        gets = parsed_gets(raw)
        if r['count'] != gets['Count'] or not math.isclose(r['mean_ms'],gets['Accumulated Latency']/gets['Count']):
            raise AssertionError('latency/count mismatch')
        for field, percentile in (('p50_ms','p50.00'),('p95_ms','p95.00'),('p99_ms','p99.00')):
            if r[field] != gets['Percentile Latencies'][percentile]:
                raise AssertionError('percentile mismatch')
        if r['ops_per_second'] != gets['Ops/sec'] or r['max_ms'] != gets['Max Latency']:
            raise AssertionError('throughput/max mismatch')
        if not math.isclose(r['cpu_seconds'],r['after']['cpu_seconds']-r['before']['cpu_seconds'],abs_tol=1e-9):
            raise AssertionError('CPU snapshots mismatch')
        if r['verified_count'] != summary['datasets'][r['dataset']]['keys']:
            raise AssertionError('incomplete integrity check')
        if r['phase'] == 'packed' and int(r['before']['info']['compressed_keys']) != r['total_keys']:
            raise AssertionError('packed phase measured before full compaction')
        if not math.isclose(r['cpu_us_per_get'],r['cpu_seconds']/r['count']*1e6):
            raise AssertionError('CPU formula mismatch')
    print(f'Checked {expected} records, datasets, counts, zero misses/errors, integrity, coverage and formulas.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path)
    parser.add_argument('--candidate', type=Path, default=ROOT/'target/release/crabcache')
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--seconds', type=int, default=4)
    parser.add_argument('--output', type=Path, default=ROOT/'docs/benchmark-results'/time.strftime('%Y-%m-%d-crabpack-latency-%H%M%S'))
    parser.add_argument('--check', type=Path)
    args = parser.parse_args()
    if args.check:
        check(json.loads((args.check/'summary.json').read_text()), args.check)
        return
    if not args.baseline or args.repeats < 1 or args.seconds < 1:
        parser.error('--baseline is required; repeats/seconds must be positive')
    binaries = {'baseline': args.baseline.resolve(), 'candidate': args.candidate.resolve()}
    if not all(b.is_file() for b in binaries.values()):
        parser.error('build both release binaries first')
    args.output.mkdir(parents=True, exist_ok=False)
    summary = {'platform': platform.platform(), 'purpose': 'local stress, no Fly CPU quota; one shard emphasizes mutex contention',
               'repeats': args.repeats, 'seconds': args.seconds, 'records': [], 'datasets': {},
               'memtier_version': subprocess.check_output(['memtier_benchmark','--version'],text=True).strip(),
               'git_commit': subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
               'source_diff_sha256': hashlib.sha256(subprocess.check_output(['git','diff','HEAD','--','src'],cwd=ROOT)).hexdigest()}

    with zipfile.ZipFile(args.output/'datasets.zip','w',compression=zipfile.ZIP_DEFLATED) as archive:
        for kind in ('small-json','large-json'):
            items = dataset(kind)
            if kind == 'large-json':
                training = [(b'session:train:'+str(i).encode(), v) for i, (_, v) in enumerate(dataset('small-json')[:2000], 1)]
                training_raw = b''.join(encode('SET',k,v) for k,v in training)
                archive.writestr('large-json-training.resp', training_raw)
                summary['large_training_sha256'] = hashlib.sha256(training_raw).hexdigest()
            raw = b''.join(encode('SET',k,v) for k,v in items)
            archive.writestr(kind+'.resp',raw)
            summary['datasets'][kind] = {'keys':len(items),'mean_value_bytes':sum(len(v) for _,v in items)/len(items),
                                         'sha256':hashlib.sha256(raw).hexdigest()}
            for repeat in range(1,args.repeats+1):
                order = ('baseline','candidate') if repeat%2 else ('candidate','baseline')
                for name in order:
                    summary['records'].extend(run_one(binaries[name],name,kind,repeat,items,args.seconds,args.output))
                    (args.output/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    check(summary,args.output)
    print(f'Artifacts: {args.output}')


if __name__ == '__main__':
    main()
