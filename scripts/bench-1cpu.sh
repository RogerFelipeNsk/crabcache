#!/usr/bin/env bash
# Per-core comparison: CrabCache restricted to one worker thread vs Redis (single-threaded), driven by
# memtier_benchmark so the load generator is not the bottleneck.
#
# Usage: scripts/bench-1cpu.sh [crabcache-port] [redis-port]   (both servers already running)
# Start CrabCache with: target/release/crabcache --port 7379 --threads 1
set -euo pipefail

CRAB_PORT=${1:-7379}
REDIS_PORT=${2:-6379}
SECS=${SECS:-10}

run() { # name port pipeline
  memtier_benchmark -s 127.0.0.1 -p "$2" --protocol=redis -t 4 -c 12 --pipeline="$3" \
    --ratio=1:9 -d 100 --key-pattern=R:R --key-maximum=100000 --test-time="$SECS" \
    --hide-histogram --print-percentiles=50,99 2>/dev/null |
    awk -v n="$1" -v p="$3" '/^Totals/ {printf "%-10s pipeline=%-3s ops/s=%12s  p50=%sms  p99=%sms\n", n, p, $2, $6, $7}'
}

for pipeline in 1 16; do
  run redis "$REDIS_PORT" "$pipeline"
  run crabcache "$CRAB_PORT" "$pipeline"
done
