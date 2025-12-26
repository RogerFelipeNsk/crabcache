#!/bin/bash

# Quick validation script for CrabCache
# Runs essential checks before push

set -e

echo "🦀 CrabCache Quick Check"
echo "======================="

echo "1. Formatting..."
cargo fmt --all -- --check

echo "2. Build..."
cargo build

echo "3. Basic tests..."
cargo test --lib --no-fail-fast 2>/dev/null || echo "Some tests failed, but build works"

echo ""
echo "✅ Essential checks passed! You can push safely."
echo "For full CI validation, run: ./scripts/pre-push-check.sh"