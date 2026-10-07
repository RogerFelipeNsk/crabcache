#!/usr/bin/env bash
# Memory per key: loads the same keys into fresh Redis and CrabCache instances and compares the
# physical footprint (macOS `footprint`, which counts compressed pages; Linux: RSS from /proc).
#
# Usage: scripts/bench-memory.sh            (needs redis-server, redis-cli and a release build)
set -euo pipefail

BIN=${BIN:-target/release/crabcache}
REDIS_PORT=${REDIS_PORT:-16379}
CRAB_PORT=${CRAB_PORT:-17379}
TMP=$(mktemp -d)
trap 'redis-cli -p "$REDIS_PORT" shutdown nosave >/dev/null 2>&1 || true; kill "$CRAB_PID" 2>/dev/null || true; rm -rf "$TMP"' EXIT

phys_kb() {
  if [ "$(uname)" = Darwin ]; then
    footprint -p "$1" 2>/dev/null | awk '/phys_footprint:/ {v=$2; u=$3; if (u=="MB") v*=1024; if (u=="GB") v*=1048576; print int(v); exit}'
  else
    awk '/VmRSS/ {print $2}' "/proc/$1/status"
  fi
}

# RESP file with N SET commands: key:<12 digits> -> value of SIZE bytes.
gen() {
  awk -v n="$1" -v size="$2" 'BEGIN {
    v = sprintf("%*s", size, ""); gsub(/ /, "x", v);
    for (i = 0; i < n; i++) {
      k = sprintf("key:%012d", i);
      printf "*3\r\n$3\r\nSET\r\n$%d\r\n%s\r\n$%d\r\n%s\r\n", length(k), k, size, v;
    }
  }' > "$TMP/load.resp"
}

printf '%-10s %-8s %14s %14s %10s\n' value keys "redis (B/key)" "crab (B/key)" "crab/redis"
for spec in "10 1000000" "100 1000000" "1000 300000"; do
  read -r size n <<< "$spec"
  gen "$n" "$size"

  redis-server --port "$REDIS_PORT" --save '' --appendonly no --daemonize yes \
    --pidfile "$TMP/redis.pid" --logfile "$TMP/redis.log"
  "$BIN" --port "$CRAB_PORT" > "$TMP/crab.log" 2>&1 &
  CRAB_PID=$!
  sleep 1
  REDIS_PID=$(cat "$TMP/redis.pid")
  r0=$(phys_kb "$REDIS_PID"); c0=$(phys_kb "$CRAB_PID")
  redis-cli -p "$REDIS_PORT" --pipe < "$TMP/load.resp" > /dev/null
  redis-cli -p "$CRAB_PORT" --pipe < "$TMP/load.resp" > /dev/null
  [ "$(redis-cli -p "$REDIS_PORT" dbsize)" = "$n" ] && [ "$(redis-cli -p "$CRAB_PORT" dbsize)" = "$n" ]
  r1=$(phys_kb "$REDIS_PID"); c1=$(phys_kb "$CRAB_PID")
  rb=$(( (r1 - r0) * 1024 / n )); cb=$(( (c1 - c0) * 1024 / n ))
  printf '%-10s %-8s %14s %14s %9s%%\n' "${size}B" "$n" "$rb" "$cb" "$(( cb * 100 / rb ))"

  redis-cli -p "$REDIS_PORT" shutdown nosave > /dev/null 2>&1 || true
  kill "$CRAB_PID"; wait "$CRAB_PID" 2>/dev/null || true
  sleep 0.5
done
