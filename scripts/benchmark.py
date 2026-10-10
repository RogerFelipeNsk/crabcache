#!/usr/bin/env python3
"""Auditable benchmarks. Standard library only; run via the bench*.sh entry points.

Fresh owned servers by default. Optional throughput ports must refer to empty,
local, disposable instances. Raw tool output, commands and summaries are retained.
"""
import argparse
import contextlib
import csv
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import socket
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


class Client:
    def __init__(self, port):
        self.socket = socket.create_connection(('127.0.0.1', port), timeout=10)
        self.file = self.socket.makefile('rb')

    def close(self):
        self.file.close()
        self.socket.close()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()

    def command(self, *args):
        self.socket.sendall(encode(args))
        return read_resp(self.file)


def encode(args):
    args = [a if isinstance(a, bytes) else str(a).encode() for a in args]
    return b'*%d\r\n' % len(args) + b''.join(b'$%d\r\n' % len(a) + a + b'\r\n' for a in args)


def read_resp(stream):
    line = stream.readline()
    require(line.endswith(b'\r\n'), 'truncated RESP header')
    kind, body = line[:1], line[1:-2]
    if kind == b'-':
        raise RuntimeError(f'server error: {body!r}')
    if kind == b'+':
        return body
    if kind == b':':
        return int(body)
    if kind == b'$':
        n = int(body)
        if n == -1:
            return None
        require(n >= 0, 'invalid bulk length')
        value = stream.read(n)
        require(len(value) == n and stream.read(2) == b'\r\n', 'truncated RESP bulk')
        return value
    if kind == b'*':
        n = int(body)
        require(n >= 0, 'invalid array length')
        return [read_resp(stream) for _ in range(n)]
    raise RuntimeError(f'unsupported RESP: {line!r}')


def dataset_info(path, expected):
    """Use RESP byte lengths, never text line lengths (UTF-8 and CRLF matter)."""
    count = total = 0
    samples = []
    sample_indexes = {0, expected - 1, *(i * expected // 32 for i in range(32))}
    digest = hashlib.sha256()
    with path.open('rb') as f:
        while f.peek(1):
            args = read_resp(f)
            require(len(args) == 3 and args[0] == b'SET', 'dataset must contain SET commands')
            key, value = args[1:]
            total += len(value)
            digest.update(encode(args))
            if count in sample_indexes:
                samples.append((key, value))
            count += 1
    require(count == expected, f'dataset count {count} != {expected}')
    return {'keys': count, 'value_bytes': total, 'mean_value_bytes': total / count,
            'sha256': digest.hexdigest()}, samples


def parse_memtier(data):
    stats = data['ALL STATS']
    totals = stats['Totals']
    require(math.isfinite(totals['Ops/sec']) and totals['Ops/sec'] > 0, 'invalid throughput')
    runtime = stats.get('Runtime', {})
    require(str(runtime.get('Interrupted', 'false')).lower() == 'false', 'interrupted run')
    for row in ('Sets', 'Gets', 'Totals'):
        require(stats[row].get('Connection Errors', 0) == 0, 'memtier connection errors')
        for key, value in stats[row].items():
            if 'error' in key.lower() and isinstance(value, (float, int)):
                require(value == 0, f'memtier {key}: {value}')
    hits, misses = stats['Gets']['Hits/sec'], stats['Gets']['Misses/sec']
    require(hits > 0 and misses == 0, f'GET workload has misses: hits/s={hits}, misses/s={misses}')
    pct = totals['Percentile Latencies']
    require(all(math.isfinite(pct[k]) and pct[k] >= 0 for k in ('p50.00', 'p99.00')),
            'invalid latency')
    return {'ops_s': totals['Ops/sec'], 'p50_ms': pct['p50.00'], 'p99_ms': pct['p99.00'],
            'hits_s': hits, 'misses_s': misses}


def parse_redis_benchmark(text):
    rows = list(csv.DictReader(text.splitlines()))
    require(len(rows) == 2 and {r['test'] for r in rows} == {'SET', 'GET'}, 'missing SET/GET results')
    result = {}
    for row in rows:
        rps = float(row['rps'])
        require(math.isfinite(rps) and rps > 0, 'invalid throughput')
        p50, p99 = float(row['p50_latency_ms']), float(row['p99_latency_ms'])
        require(math.isfinite(p50) and math.isfinite(p99) and 0 <= p50 <= p99, 'invalid latency')
        result[row['test']] = {'ops_s': rps, 'p50_ms': p50, 'p99_ms': p99}
    return result


def validate_get_counts(server_hits, client_count, connections, pipeline):
    # A timed run may close connections with replies still in flight. The server
    # has executed those GETs; memtier only counts the replies it consumed.
    unreported = server_hits - client_count
    require(0 <= unreported <= connections * pipeline,
            f'GET count mismatch beyond in-flight bound: server {server_hits}, client {client_count}')
    return unreported


def physical_bytes(pid):
    if sys.platform == 'darwin':
        out = subprocess.check_output(['footprint', '-p', str(pid)], text=True, stderr=subprocess.STDOUT)
        m = re.search(r'phys_footprint:\s*([\d.]+)\s*(B|KB|MB|GB)', out)
        require(m is not None, f'footprint missing for pid {pid}: {out[-500:]}')
        return round(float(m[1]) * {'B': 1, 'KB': 1024, 'MB': 1024**2, 'GB': 1024**3}[m[2]])
    text = Path(f'/proc/{pid}/status').read_text()
    m = re.search(r'^VmRSS:\s*(\d+)\s+kB', text, re.M)
    require(m is not None, f'RSS missing for pid {pid}')
    return int(m[1]) * 1024


def cpu_seconds(pid):
    # Cumulative process CPU, not ps %cpu (which averages over process lifetime).
    text = subprocess.check_output(['ps', '-p', str(pid), '-o', 'time='], text=True).strip()
    days = 0
    if '-' in text:
        day, text = text.split('-', 1)
        days = int(day)
    parts = [float(p) for p in text.split(':')]
    return days * 86400 + sum(p * 60**i for i, p in enumerate(reversed(parts)))


class Bench:
    def __init__(self, args):
        self.args = args
        self.out = Path(os.getenv('OUT_DIR', str(ROOT / 'target' / 'benchmarks' /
                        (time.strftime('%Y%m%d-%H%M%S') + f'-{args.mode}-{os.getpid()}')))).resolve()
        self.out.mkdir(parents=True, exist_ok=False)
        self.results = []
        self.serial = 0
        self.bin = str(Path(os.getenv('BIN', str(ROOT / 'target/release/crabcache'))).resolve())
        self.secs = int(os.getenv('SECS', '10'))
        self.repeats = int(os.getenv('REPEATS', '3'))
        require(self.secs > 0 and self.repeats > 0, 'SECS and REPEATS must be positive')
        self.metadata = {'platform': platform.platform(), 'machine': platform.machine(),
                         'cpu_count': os.cpu_count(), 'mode': args.mode,
                         'seconds': self.secs, 'repeats': self.repeats,
                         'memory_metric': 'physical_footprint' if sys.platform == 'darwin' else 'RSS',
                         'git_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                         'git_dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT)),
                         'binary_sha256': hashlib.sha256(Path(self.bin).read_bytes()).hexdigest()}
        if sys.platform == 'darwin':
            self.metadata['cpu_model'] = subprocess.check_output(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip()
            self.metadata['memory_bytes'] = int(subprocess.check_output(['sysctl', '-n', 'hw.memsize'], text=True))
        versions = [('redis', ['redis-server', '--version']), ('crabcache', [self.bin, '--version'])]
        if args.mode in ('core', 'compression'):
            versions.append(('memtier', ['memtier_benchmark', '--version']))
        if args.mode == 'default':
            versions.append(('redis_benchmark', ['redis-benchmark', '--version']))
        for name, cmd in versions:
            self.metadata[name] = subprocess.check_output(cmd, text=True, stderr=subprocess.STDOUT).strip()
        self.save()
        print(f'Raw artifacts: {self.out}', flush=True)

    def save(self):
        (self.out / 'summary.json').write_text(json.dumps({'metadata': self.metadata, 'results': self.results}, indent=2) + '\n')

    def run(self, command, label, stdin=None, timeout=300):
        self.serial += 1
        base = self.out / f'{self.serial:03d}-{label}'
        base.with_suffix('.command.json').write_text(json.dumps(command) + '\n')
        with base.with_suffix('.stdout').open('wb') as stdout, base.with_suffix('.stderr').open('wb') as stderr:
            p = subprocess.run(command, stdin=stdin, stdout=stdout, stderr=stderr, timeout=timeout)
        require(p.returncode == 0, f'{label} failed ({p.returncode}); see {base}.stderr')
        return base.with_suffix('.stdout').read_text()

    @contextlib.contextmanager
    def server(self, name, threads=None, packed=False, port=None):
        process = None
        log = None
        if port is None:
            # Port 0 is unavailable in redis-server (disables TCP), so probe a free port.
            with socket.socket() as probe:
                probe.bind(('127.0.0.1', 0))
                port = probe.getsockname()[1]
            command = (['redis-server', '--bind', '127.0.0.1', '--port', str(port), '--save', '', '--appendonly', 'no']
                       if name == 'redis' else [self.bin, '--bind', '127.0.0.1', '--port', str(port)])
            if name != 'redis':
                if threads:
                    command += ['--threads', str(threads)]
                if packed:
                    command += ['--compression', '--compression-min-idle', '0']
            self.serial += 1
            log = (self.out / f'{self.serial:03d}-{name}-server.log').open('wb')
            (self.out / f'{self.serial:03d}-{name}-server.command.json').write_text(json.dumps(command) + '\n')
            # User shell configuration must not silently change a benchmark server.
            env = {k: v for k, v in os.environ.items() if not k.startswith('CRABCACHE_')}
            process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, env=env)
        try:
            deadline = time.monotonic() + 10
            while True:
                require(process is None or process.poll() is None, 'server exited before readiness')
                try:
                    client = Client(port)
                    break
                except OSError:
                    require(time.monotonic() < deadline, 'server readiness timed out')
                    time.sleep(.05)
            with client:
                require(client.command('PING') == b'PONG', 'PING did not return PONG')
                info = self.info(client)
                pid = int(info['process_id'])
                require(process is None or process.pid == pid, 'port belongs to another process')
                require(('crabcache_version' in info) == (name != 'redis'), 'wrong server identity')
                require(client.command('DBSIZE') == 0, 'benchmark requires an empty disposable server')
                if threads and name != 'redis':
                    require(int(info['io_threads_active']) == threads, f'expected --threads {threads}')
                yield client, pid, port
        finally:
            if process is not None:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                log.close()

    @staticmethod
    def info(client):
        return dict(line.split(':', 1) for line in client.command('INFO').decode().splitlines()
                    if ':' in line and not line.startswith('#'))

    def load(self, client, port, path, n, samples):
        with path.open('rb') as f:
            out = self.run(['redis-cli', '-h', '127.0.0.1', '-p', str(port), '--pipe'], 'load', stdin=f)
        m = re.search(r'errors:\s*(\d+), replies:\s*(\d+)', out)
        require(m is not None and int(m[1]) == 0 and int(m[2]) == n, f'invalid load: {out}')
        require(client.command('DBSIZE') == n, 'DBSIZE differs from dataset')
        self.verify(client, samples)

    @staticmethod
    def verify(client, samples):
        for key, expected in samples:
            require(client.command('GET', key) == expected, f'value mismatch for {key!r}')

    @staticmethod
    def verify_all(client, path):
        """Independent full readback, outside timing, in bounded pipeline batches."""
        with path.open('rb') as f:
            while f.peek(1):
                batch = []
                for _ in range(256):
                    if not f.peek(1):
                        break
                    _, key, expected = read_resp(f)
                    batch.append((key, expected))
                client.socket.sendall(b''.join(encode(['GET', key]) for key, _ in batch))
                for key, expected in batch:
                    require(read_resp(client.file) == expected, f'full readback mismatch for {key!r}')

    def dataset(self, kind, n, size=None, prefix=None):
        path = self.out / f'{kind}-{n}-{size}.resp'
        if size is None:
            command = [str(ROOT / 'target/release/examples/dataset'), kind, str(n)]
            with path.open('wb') as f:
                subprocess.run(command, stdout=f, check=True)
        else:
            with path.open('wb') as f:
                for i in range(n):
                    key = f'key:{i:012d}' if prefix is None else f'{prefix}{i}'
                    f.write(encode(['SET', key, b'x' * size]))
        info, samples = dataset_info(path, n)
        (self.out / f'{path.stem}.dataset.json').write_text(json.dumps(info, indent=2) + '\n')
        return path, info, samples

    def pack(self, client, n, pid, memory_samples=None):
        deadline = time.monotonic() + float(os.getenv('PACK_TIMEOUT', '120'))
        while True:
            info = self.info(client)
            if memory_samples is not None:
                memory_samples.append(physical_bytes(pid))
            packed = int(info['compressed_keys'])
            if packed >= n:
                return info
            require(time.monotonic() < deadline, f'compression timeout: {packed}/{n} keys; result rejected')
            time.sleep(.2)

    def record(self, result):
        self.results.append(result)
        self.save()
        print(json.dumps(result), flush=True)

    def memtier(self, client, pid, port, pipeline, prefix, n, ratio, label):
        raw = self.out / f'{len(self.results):03d}-{label}-p{pipeline}.memtier.json'
        command = ['memtier_benchmark', '-s', '127.0.0.1', '-p', str(port), '--protocol=redis',
                   '-t', '4', '-c', '12', f'--pipeline={pipeline}', f'--ratio={ratio}', '-d', '100',
                   f'--key-prefix={prefix}', '--key-minimum=1', f'--key-maximum={n - 1}',
                   '--key-pattern=R:R', '--distinct-client-seed', f'--test-time={self.secs}',
                   '--hide-histogram', '--print-percentiles=50,99', f'--json-out-file={raw}']
        before = self.info(client)
        cpu0, wall0 = cpu_seconds(pid), time.monotonic()
        self.run(command, 'memtier', timeout=self.secs + 60)
        elapsed, cpu = time.monotonic() - wall0, cpu_seconds(pid) - cpu0
        after = self.info(client)
        require(int(after['keyspace_misses']) == int(before['keyspace_misses']), 'server reports GET misses')
        data = json.loads(raw.read_text())
        result = parse_memtier(data)
        get_count = data['ALL STATS']['Gets']['Count']
        server_hits = int(after['keyspace_hits']) - int(before['keyspace_hits'])
        unreported = validate_get_counts(server_hits, get_count, 48, pipeline)
        require(cpu > 0, 'CPU timer did not advance')
        result.update(cpu_seconds=cpu, wall_seconds=elapsed, mean_cpu_cores=cpu / elapsed,
                      ops_count=data['ALL STATS']['Totals']['Count'],
                      ops_per_cpu_second=data['ALL STATS']['Totals']['Count'] / cpu,
                      server_get_hits=server_hits, client_get_count=get_count,
                      unreported_gets=unreported, max_unreported_gets=48 * pipeline,
                      measured_key_minimum=1, measured_key_maximum=n - 1)
        return result

    def memory(self, compression=False):
        n_json = self.args.keys or 300000
        specs = [('session', n_json, None), ('product', n_json, None), ('api', n_json, None)] if compression else [
            ('fixed', int(os.getenv('N', '1000000')), 10), ('fixed', int(os.getenv('N', '1000000')), 100),
            ('fixed', int(os.getenv('LARGE_N', '300000')), 1000)]
        for kind, n, size in specs:
            require(n > 0, 'key count must be positive')
            path, data, samples = self.dataset(kind, n, size)
            targets = ['redis', 'crabcache', 'crabpack'] if compression else ['redis', 'crabcache']
            for repeat in range(self.repeats):
                for name in targets if repeat % 2 == 0 else targets[::-1]:
                    with self.server(name, packed=name == 'crabpack') as (client, pid, port):
                        base = physical_bytes(pid)
                        self.load(client, port, path, n, samples)
                        observed = [physical_bytes(pid)]
                        packed_info = {}
                        if name == 'crabpack':
                            packed_info = self.pack(client, n, pid, observed)
                        time.sleep(2)
                        final_samples = [physical_bytes(pid) for _ in range(3)]
                        final = statistics.median(final_samples)
                        require(final > base, 'nonpositive memory delta')
                        # Verification happens after memory sampling so GET buffers do not bias the measurement.
                        self.verify_all(client, path)
                        result = {'mode': 'memory', 'dataset': kind, 'value_size': size, 'repeat': repeat + 1,
                                  'server': name, **data, 'baseline_bytes': base, 'final_bytes': final,
                                  'final_samples_bytes': final_samples, 'post_load_samples_bytes': observed,
                                  'bytes_per_key': (final - base) / n,
                                  'verified_values': n,
                                  'observed_post_load_max_bytes_per_key': (max(observed + final_samples) - base) / n}
                        if packed_info:
                            result.update(compressed_keys=int(packed_info['compressed_keys']),
                                          compressed_original_bytes=int(packed_info['compressed_original_bytes']),
                                          compressed_stored_bytes=int(packed_info['compressed_stored_bytes']),
                                          compression_ratio=float(packed_info['compression_ratio']))
                        self.record(result)
            path.unlink()  # Hash, exact size and deterministic generator retained; avoid committing huge input files.

    def throughput(self, mode):
        n = int(os.getenv('KEYSPACE', '100000'))
        require(n > 1, 'KEYSPACE must be > 1')
        size = int(os.getenv('VALUE_SIZE', '100'))
        if mode == 'compression':
            n = self.args.keys or 300000
            prefix = 'session:'
            path, data, samples = self.dataset('session', n)
        else:
            prefix = 'key:' if mode == 'default' else 'memtier-'
            path, data, samples = self.dataset('fixed', n, size, prefix=None if mode == 'default' else prefix)
        configs = [(1, 1), (50, 1), (50, 16), (50, 64)] if mode == 'default' else [(48, 1), (48, 16)]
        targets = ['redis', 'crabcache', 'crabpack'] if mode == 'compression' else ['redis', 'crabcache']
        for clients, pipeline in configs:
            for repeat in range(self.repeats):
                for name in targets if repeat % 2 == 0 else targets[::-1]:
                    port = None
                    if self.args.ports:
                        port = self.args.ports[1 if name == 'redis' else 0]
                    with self.server(name, threads=1 if mode != 'default' else None,
                                     packed=name == 'crabpack', port=port) as (client, pid, port):
                        self.load(client, port, path, n, samples)
                        if name == 'crabpack':
                            self.pack(client, n, pid)
                            self.verify(client, samples)
                        # Equal prefill plus a deterministic warmup of sampled values on every target.
                        for _ in range(10):
                            self.verify(client, samples)
                        result = {'mode': mode, 'server': name, 'repeat': repeat + 1,
                                  'clients': clients, 'pipeline': pipeline, **data}
                        if mode == 'default':
                            count = int(os.getenv('N', '2000000')) // (10 if clients == 1 else 1)
                            require(count > 0, 'request count must be positive')
                            before = self.info(client)
                            text = self.run(['redis-benchmark', '-h', '127.0.0.1', '-p', str(port), '--csv',
                                             '-t', 'set,get', '-c', str(clients), '-P', str(pipeline), '-d', str(size),
                                             '-r', str(n), '-n', str(count)], 'redis-benchmark', timeout=600)
                            result.update(parse_redis_benchmark(text))
                            after = self.info(client)
                            require(int(after['keyspace_misses']) == int(before['keyspace_misses']), 'GET misses')
                            # redis-benchmark overwrites values with its own deterministic byte pattern.
                            for key, _ in samples:
                                require(len(client.command('GET', key)) == size, 'wrong value size after benchmark')
                        else:
                            result.update(self.memtier(client, pid, port, pipeline, prefix, n,
                                                      '0:1' if mode == 'compression' else '1:9', name))
                            if mode == 'compression':
                                self.verify_all(client, path)
                                result['verified_values'] = n
                                if name == 'crabpack':
                                    result['compressed_keys_after'] = int(self.info(client)['compressed_keys'])
                                    require(result['compressed_keys_after'] == n, 'packed coverage changed')
                        require(client.command('DBSIZE') == n, 'keyspace changed during benchmark')
                        self.record(result)
                        if self.args.ports:
                            require(client.command('FLUSHDB') == b'OK', 'cleanup failed')
        path.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['default', 'core', 'memory', 'compression'])
    parser.add_argument('ports', type=int, nargs='*', help='optional CrabCache and Redis ports (empty, disposable instances)')
    parser.add_argument('--keys', type=int)
    args = parser.parse_args()
    require(len(args.ports) in (0, 2), 'provide both CrabCache and Redis ports, or neither')
    require(not args.ports or args.mode in ('default', 'core'), 'ports apply only to throughput modes')
    require(args.keys is None or args.keys > 1, 'keys must be > 1')
    subprocess.run(['cargo', 'build', '--release', '--bin', 'crabcache', '--example', 'dataset'], cwd=ROOT, check=True)
    bench = Bench(args)
    if args.mode == 'memory':
        bench.memory()
    elif args.mode == 'compression':
        if os.getenv('SKIP_MEMORY') != '1':
            bench.memory(compression=True)
        if os.getenv('SKIP_GET') != '1':
            bench.throughput('compression')
    else:
        bench.throughput(args.mode)


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, ValueError, KeyError, subprocess.SubprocessError) as exc:
        sys.exit(f'benchmark rejected: {exc}')
