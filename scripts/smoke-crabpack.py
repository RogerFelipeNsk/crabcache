#!/usr/bin/env python3
"""Exercise CrabPack on a disposable server/image with compression enabled and min-idle 0."""
import argparse
import hashlib
import json
import os
import time

from benchmark import Bench, Client, encode, read_resp, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', type=int, default=6379)
    args = parser.parse_args()
    count = 2000
    deadline_ms = int(time.time() * 1000) + 120_000
    values = [json.dumps({
        'user_id': i, 'email': f'user{i}@example.com', 'roles': ['user', 'editor'],
        'locale': 'pt-BR', 'theme': 'dark' if i % 2 else 'light',
        'last_seen': f'2026-10-{1 + i % 28:02d}T12:30:00Z',
        'cart': [{'sku': f'SKU-{i % 100}', 'qty': 1 + i % 4}],
        'csrf': hashlib.sha256(str(i).encode()).hexdigest()[:32],
    }, separators=(',', ':')).encode() + b'\0\xff\xfe\r\n' for i in range(count)]
    keys = [f'crabpack-smoke:{i}' for i in range(count)]
    with Client(args.port) as client:
        password = os.getenv('REDISCLI_AUTH')
        if password:
            require(client.command('AUTH', password) == b'OK', 'AUTH failed')
        info = Bench.info(client)
        require('crabcache_version' in info, 'target is not CrabCache')
        require(info['compression'] == 'yes' and info['compression_min_idle'] == '0',
                'smoke test requires --compression --compression-min-idle 0')
        require(client.command('DBSIZE') <= 1, 'target must be a fresh disposable instance')
        client.socket.sendall(b''.join(encode(['SET', key, value, 'PXAT', deadline_ms])
                                      for key, value in zip(keys, values)))
        for _ in keys:
            require(read_resp(client.file) == b'OK', 'SET failed')
        deadline = time.monotonic() + 45
        while True:
            packed = int(Bench.info(client)['compressed_keys'])
            if packed == count:
                break
            require(time.monotonic() < deadline, f'compression timeout: {packed}/{count}')
            time.sleep(.1)
        for key, value in zip(keys, values):
            require(client.command('GET', key) == value, f'binary value mismatch: {key}')
        require(client.command('PEXPIRETIME', keys[0]) == deadline_ms, 'packed value lost TTL')
        require(client.command('CONFIG', 'SET', 'compression', 'no') == b'OK', 'disable failed')
        require(client.command('GET', keys[-1]) == values[-1], 'packed value unreadable after disable')
        require(client.command('APPEND', keys[0], b'!') == len(values[0]) + 1, 'APPEND length mismatch')
        require(client.command('GET', keys[0]) == values[0] + b'!', 'APPEND corrupted packed value')
        require(client.command('PEXPIRETIME', keys[0]) == deadline_ms, 'APPEND lost TTL')
    print(f'CrabPack smoke passed: {count} packed binary values, exact readback, TTL, disable and APPEND')


if __name__ == '__main__':
    main()
