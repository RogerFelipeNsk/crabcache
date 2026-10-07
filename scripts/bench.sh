#!/usr/bin/env bash
# Reproducible CrabCache vs Redis comparison with the official redis-benchmark.
#
# Usage: scripts/bench.sh [crabcache-port] [redis-port]
# Both servers must already be running on this machine. Run on an otherwise idle host; the client
# shares CPU with the servers, so results are only comparable within one run.
set -euo pipefail

CRAB_PORT=${1:-7379}
REDIS_PORT=${2:-6379}
N=${N:-2000000}
VALUE_SIZE=${VALUE_SIZE:-100}
KEYSPACE=${KEYSPACE:-100000}

for p in "$CRAB_PORT" "$REDIS_PORT"; do
  redis-cli -p "$p" ping > /dev/null || { echo "nothing answering on port $p" >&2; exit 1; }
done

echo "redis-benchmark: SET/GET, ${VALUE_SIZE}B values, ${KEYSPACE} keys, $(uname -sm)"
printf '%-10s %-6s %-9s %14s %10s %14s %10s\n' server clients pipeline "SET rps" "SET p50" "GET rps" "GET p50"
for cfg in "1 1" "50 1" "50 16" "50 64"; do
  read -r clients pipeline <<< "$cfg"
  n=$N
  [ "$clients" = 1 ] && n=$((N / 10))
  for target in "redis $REDIS_PORT" "crabcache $CRAB_PORT"; do
    read -r name port <<< "$target"
    out=$(redis-benchmark -p "$port" -q -t set,get -c "$clients" -P "$pipeline" -d "$VALUE_SIZE" \
      -r "$KEYSPACE" -n "$n" 2>&1 | tr '\r' '\n' | grep 'per second')
    set_rps=$(echo "$out" | awk '/^SET/ {print $2}')
    set_p50=$(echo "$out" | awk -F'p50=' '/^SET/ {print $2}' | awk '{print $1}')
    get_rps=$(echo "$out" | awk '/^GET/ {print $2}')
    get_p50=$(echo "$out" | awk -F'p50=' '/^GET/ {print $2}' | awk '{print $1}')
    printf '%-10s %-6s %-9s %14s %10s %14s %10s\n' "$name" "$clients" "$pipeline" "$set_rps" "${set_p50}ms" "$get_rps" "${get_p50}ms"
  done
done
