#!/usr/bin/env bash
# Synthetic JSON, validated load and 100% compression before measuring.
# REPEATS=3 SECS=10 SKIP_MEMORY=1 or SKIP_GET=1 PACK_TIMEOUT=120 OUT_DIR=<new directory>
set -euo pipefail
exec python3 "$(dirname "$0")/benchmark.py" compression --keys "${1:-300000}"
