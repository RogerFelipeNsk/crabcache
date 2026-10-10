#!/usr/bin/env bash
# Fresh owned servers; physical footprint on macOS, RSS on Linux. Baseline subtracted.
# REPEATS=3 N=1000000 LARGE_N=300000 OUT_DIR=<new directory>
set -euo pipefail
exec python3 "$(dirname "$0")/benchmark.py" memory "$@"
