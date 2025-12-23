#!/bin/bash
# Benchmark script for CrabCache

echo "CrabCache Benchmark Script"
echo "=========================="

# Check if server is running
if ! nc -z 127.0.0.1 7000; then
    echo "Error: CrabCache server is not running on port 7000"
    echo "Start the server with: cargo run"
    exit 1
fi

echo "Server is running, starting benchmarks..."

# Run Rust benchmarks
echo "Running Rust benchmarks..."
cargo bench

echo "Benchmark completed!"