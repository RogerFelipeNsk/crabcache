#!/usr/bin/env bash
# One CrabCache worker vs Redis; actual process CPU is recorded (no CPU affinity claim).
# Fresh servers by default; optional args: empty disposable CrabCache/Redis ports.
# SECS=10 REPEATS=3 KEYSPACE=100000 OUT_DIR=<new directory>
set -euo pipefail
exec python3 "$(dirname "$0")/benchmark.py" core "$@"
