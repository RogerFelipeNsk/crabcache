# CrabCache Makefile
# Provides convenient aliases for development tasks

.PHONY: help check check-full fmt build test clean install-deps

# Default target
help:
	@echo "CrabCache Development Commands"
	@echo "============================="
	@echo ""
	@echo "Quick Development:"
	@echo "  make check      - Quick validation (fmt + build + basic tests)"
	@echo "  make fmt        - Format code"
	@echo "  make build      - Build project"
	@echo "  make test       - Run tests"
	@echo ""
	@echo "Pre-Push Validation:"
	@echo "  make check-full - Complete CI validation"
	@echo ""
	@echo "Utilities:"
	@echo "  make clean      - Clean build artifacts"
	@echo "  make install-deps - Install optional development dependencies"
	@echo ""

# Quick validation - same as quick-check.sh
check:
	@echo "🦀 Running quick validation..."
	@./scripts/quick-check.sh

# Full CI validation - same as pre-push-check.sh  
check-full:
	@echo "🦀 Running full CI validation..."
	@./scripts/pre-push-check.sh

# Format code
fmt:
	@echo "🎨 Formatting code..."
	@cargo fmt --all

# Build project
build:
	@echo "🔨 Building project..."
	@cargo build

# Build release
build-release:
	@echo "🔨 Building release..."
	@cargo build --release

# Run tests
test:
	@echo "🧪 Running tests..."
	@cargo test --lib

# Run all tests including integration
test-all:
	@echo "🧪 Running all tests..."
	@cargo test

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	@cargo clean

# Install optional development dependencies
install-deps:
	@echo "📦 Installing development dependencies..."
	@cargo install cargo-audit || echo "cargo-audit already installed or failed"
	@cargo install cargo-watch || echo "cargo-watch already installed or failed"
	@echo "✅ Dependencies installed"

# Lint with clippy
lint:
	@echo "🔍 Running clippy..."
	@cargo clippy --all-targets --all-features

# Generate documentation
docs:
	@echo "📚 Generating documentation..."
	@cargo doc --no-deps --document-private-items --open

# Watch for changes and run tests
watch:
	@echo "👀 Watching for changes..."
	@cargo watch -x "test --lib"

# Security audit
audit:
	@echo "🔒 Running security audit..."
	@cargo audit

# Run benchmarks
bench:
	@echo "⚡ Running benchmarks..."
	@cargo bench

# Setup git hooks
setup-hooks:
	@echo "🪝 Setting up git hooks..."
	@echo '#!/bin/bash\nmake check' > .git/hooks/pre-push
	@chmod +x .git/hooks/pre-push
	@echo "✅ Pre-push hook installed"