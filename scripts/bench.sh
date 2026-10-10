#!/usr/bin/env bash
# Fresh local servers by default; optional args: empty disposable CrabCache/Redis ports.
# REPEATS=3 N=2000000 VALUE_SIZE=100 KEYSPACE=100000 OUT_DIR=<new directory>
set -euo pipefail
exec python3 "$(dirname "$0")/benchmark.py" default "$@"
