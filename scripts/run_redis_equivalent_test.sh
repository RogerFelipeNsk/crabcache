#!/bin/bash

# CrabCache Redis-Equivalent Test Runner
# This script runs the Redis-equivalent benchmark to compare CrabCache with Redis

echo "🚀 CrabCache Redis-Equivalent Test Runner"
echo "=========================================="
echo ""

# Check if CrabCache server is running
echo "🔍 Checking if CrabCache server is running on port 7001..."
if ! nc -z 127.0.0.1 7001 2>/dev/null; then
    echo "❌ CrabCache server is not running on port 7001"
    echo "💡 Please start the server first:"
    echo "   cd crabcache && cargo run --release"
    exit 1
fi

echo "✅ CrabCache server is running"
echo ""

# Run the Redis-equivalent benchmark
echo "⚡ Running Redis-equivalent benchmark..."
echo "📊 Settings: 100 connections, 1M requests, pipeline=16, 64-byte data"
echo ""

cd crabcache
python3 scripts/redis_equivalent_test.py

echo ""
echo "🎯 Next Steps:"
echo "1. Compare results with actual Redis:"
echo "   redis-benchmark -h 127.0.0.1 -p 6379 -c 100 -n 1000000 -d 64 -P 16 -t ping,set,get"
echo ""
echo "2. If CrabCache achieves 100k+ ops/sec, we've surpassed Redis!"
echo "3. If not, implement true pipelining in the server for 10x improvement"
echo ""
echo "📈 Expected Results:"
echo "   - Without pipelining: ~25k ops/sec (current)"
echo "   - With pipelining: ~250k+ ops/sec (target)"
echo "   - Redis baseline: ~37k ops/sec"