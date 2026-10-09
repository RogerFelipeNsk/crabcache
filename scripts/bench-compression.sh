#!/usr/bin/env bash
# CrabPack benchmark on realistic JSON datasets (examples/dataset.rs):
#   1. memory per key: Redis vs CrabCache without compression vs CrabCache with compression
#   2. GET throughput/latency on compressed vs plain values, one server thread each (memtier)
#
# Usage: scripts/bench-compression.sh [keys-per-dataset]   (needs redis-server, redis-cli, memtier_benchmark)
#        SKIP_MEMORY=1 runs only the GET benchmark.
set -euo pipefail

N=${1:-300000}
BIN=${BIN:-target/release/crabcache}
GEN=target/release/examples/dataset
PORT=${PORT:-17400}
TMP=$(mktemp -d)
SERVER_PID=""
trap 'stop_server; rm -rf "$TMP"' EXIT

cargo build --release --quiet --bin crabcache --example dataset

phys_kb() {
  if [ "$(uname)" = Darwin ]; then
    footprint -p "$1" 2>/dev/null | awk '/phys_footprint:/ {v=$2; if ($3=="MB") v*=1024; if ($3=="GB") v*=1048576; print int(v); exit}'
  else
    awk '/VmRSS/ {print $2}' "/proc/$1/status"
  fi
}

stop_server() {
  if [ -n "$SERVER_PID" ]; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    SERVER_PID=""
  fi
}

start_server() { # name [crabcache flags...]
  local name=$1
  shift
  if [ "$name" = redis ]; then
    redis-server --port "$PORT" --save '' --appendonly no > "$TMP/server.log" 2>&1 &
  else
    "$BIN" --port "$PORT" "$@" > "$TMP/server.log" 2>&1 &
  fi
  SERVER_PID=$!
  for _ in $(seq 1 50); do redis-cli -p "$PORT" ping > /dev/null 2>&1 && return; sleep 0.1; done
  echo "server did not start" >&2
  exit 1
}

field() { redis-cli -p "$PORT" info "$1" | tr -d '\r' | awk -F: -v k="$2" '$1==k {print $2}'; }

# Loads the dataset, waits for compression when enabled; prints "bytes_per_key peak_bytes_per_key".
load() {
  local kind=$1 packed=$2
  local base peak now
  base=$(phys_kb "$SERVER_PID")
  redis-cli -p "$PORT" --pipe < "$TMP/$kind.resp" > /dev/null
  peak=$(phys_kb "$SERVER_PID")
  if [ "$packed" = yes ]; then
    for _ in $(seq 1 600); do
      now=$(phys_kb "$SERVER_PID")
      [ "$now" -gt "$peak" ] && peak=$now
      [ "$(field compression compressed_keys)" -ge $((N * 99 / 100)) ] && break
      sleep 0.2
    done
    sleep 2
  fi
  now=$(phys_kb "$SERVER_PID")
  [ "$now" -gt "$peak" ] && peak=$now
  echo "$(( (now - base) * 1024 / N )) $(( (peak - base) * 1024 / N ))"
}

for kind in session product api; do
  "$GEN" "$kind" "$N" > "$TMP/$kind.resp"
done

if [ "${SKIP_MEMORY:-0}" != 1 ]; then
echo "== Memory per key ($N keys per dataset, $(uname -sm))"
printf '%-9s %10s %10s %10s %14s %14s\n' dataset value redis crab crab+pack "pack vs redis"
for kind in session product api; do
  avg=$(awk 'NR % 7 == 0 {s += length($0); n++} END {printf "%d", s / n}' "$TMP/$kind.resp")
  start_server redis; read -r redis _ <<< "$(load "$kind" no)"; stop_server
  start_server crab; read -r plain _ <<< "$(load "$kind" no)"; stop_server
  start_server crab --compression --compression-min-idle 0
  read -r packed peak <<< "$(load "$kind" yes)"
  ratio=$(field compression compression_ratio)
  stop_server
  printf '%-9s %9sB %9sB %9sB %9sB (x%s) %8s%%   peak during packing: %sB\n' \
    "$kind" "$avg" "$redis" "$plain" "$packed" "$ratio" "$(( packed * 100 / redis ))" "$peak"
done
echo
fi

echo "== GET on session:* values, one server thread each (memtier, 4x12 clients, 10s)"
run_get() { # label pipeline
  memtier_benchmark -s 127.0.0.1 -p "$PORT" --protocol=redis -t 4 -c 12 --pipeline="$2" --ratio=0:1 \
    --key-prefix=session: --key-minimum=1 --key-maximum=$((N - 1)) --key-pattern=R:R --test-time=10 \
    --hide-histogram --print-percentiles=50,99 2>/dev/null |
    awk -v l="$1" -v p="$2" '/^Totals/ {printf "%-22s pipeline=%-3s ops/s=%12s  p50=%sms  p99=%sms\n", l, p, $2, $6, $7}'
}
for pipeline in 1 16; do
  start_server redis; load session no > /dev/null; run_get "redis" "$pipeline"; stop_server
  start_server crab --threads 1; load session no > /dev/null; run_get "crabcache plain" "$pipeline"; stop_server
  start_server crab --threads 1 --compression --compression-min-idle 0
  load session yes > /dev/null
  run_get "crabcache compressed" "$pipeline"
  stop_server
done
