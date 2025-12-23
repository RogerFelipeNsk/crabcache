#!/bin/bash

echo "🎯 CrabCache P99 < 1ms Test Suite"
echo "=================================="
echo ""

# Check if server is running
echo "🔧 Checking server status..."
if docker ps | grep -q "crabcache-server"; then
    echo "✅ CrabCache server is running on port 7001"
else
    echo "❌ CrabCache server is not running"
    echo "💡 Start with: docker-compose up -d"
    exit 1
fi

# Test basic connectivity
echo ""
echo "🔧 Testing connectivity..."
if echo -e '\x01' | nc -w 1 127.0.0.1 7001 | xxd | grep -q "11"; then
    echo "✅ Server is responding correctly"
else
    echo "❌ Server is not responding"
    exit 1
fi

echo ""
echo "🚀 Running P99 < 1ms Tests..."
echo "=============================="

# Test 1: Ultra Low Latency Benchmark (comprehensive)
echo ""
echo "📊 Test 1: Ultra Low Latency Benchmark"
echo "--------------------------------------"
python3 crabcache/scripts/ultra_low_latency_benchmark.py

# Test 2: P99 Validation Demo (multiple scenarios)
echo ""
echo "📊 Test 2: P99 Validation Demo"
echo "------------------------------"
python3 crabcache/scripts/p99_validation_demo.py

# Test 3: Quick Single Connection Test
echo ""
echo "📊 Test 3: Quick Single Connection Test"
echo "---------------------------------------"
python3 -c "
import socket
import struct
import time
import numpy as np

def quick_p99_test():
    print('🔬 Quick P99 test (single connection, 10,000 ops)...')
    
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    sock.connect(('127.0.0.1', 7001))
    
    latencies = []
    CMD_PING = 0x01
    RESP_PONG = 0x11
    
    for i in range(10000):
        start = time.perf_counter_ns()
        sock.send(bytes([CMD_PING]))
        response = sock.recv(1)
        end = time.perf_counter_ns()
        
        if len(response) == 1 and response[0] == RESP_PONG:
            latency_ms = (end - start) / 1_000_000
            latencies.append(latency_ms)
    
    sock.close()
    
    if latencies:
        p50 = np.percentile(latencies, 50)
        p95 = np.percentile(latencies, 95)
        p99 = np.percentile(latencies, 99)
        
        print(f'📊 Quick Results ({len(latencies):,} ops):')
        print(f'  P50: {p50:.3f}ms')
        print(f'  P95: {p95:.3f}ms')
        print(f'  P99: {p99:.3f}ms')
        
        if p99 < 1.0:
            print(f'✅ P99 < 1ms ACHIEVED! ({p99:.3f}ms)')
            margin = 1.0 - p99
            print(f'🎯 Margin: {margin:.3f}ms under target')
        else:
            print(f'❌ P99 goal not achieved ({p99:.3f}ms > 1.0ms)')
    else:
        print('❌ No successful operations')

quick_p99_test()
"

echo ""
echo "🎉 P99 Test Suite Complete!"
echo ""
echo "📁 Results saved in: crabcache/benchmark_results/"
echo "📊 Key files:"
echo "  - ultra_low_latency_results_*.json"
echo "  - p99_validation_*.json"
echo "  - ultra_low_latency_success_report.md"