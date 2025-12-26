#!/bin/bash

# Pre-push validation script for CrabCache
# This script runs the same checks as the CI pipeline to ensure everything passes before pushing

set -e  # Exit on any error

echo "🦀 CrabCache Pre-Push Validation"
echo "================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print status
print_status() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✅ $2${NC}"
    else
        echo -e "${RED}❌ $2${NC}"
        exit 1
    fi
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

echo "Step 1: Check formatting..."
cargo fmt --all -- --check
print_status $? "Code formatting"

echo ""
echo "Step 2: Run Clippy (linting)..."
cargo clippy --all-targets --all-features -- -D warnings
print_status $? "Clippy linting"

echo ""
echo "Step 3: Build project..."
cargo build --release
print_status $? "Release build"

echo ""
echo "Step 4: Run tests..."
cargo test --lib --release
print_status $? "Unit tests"

echo ""
echo "Step 5: Run integration tests..."
cargo test --test '*' --release
print_status $? "Integration tests"

echo ""
echo "Step 6: Check documentation..."
cargo doc --no-deps --document-private-items
print_status $? "Documentation generation"

echo ""
echo "Step 7: Security audit..."
if command -v cargo-audit &> /dev/null; then
    cargo audit
    print_status $? "Security audit"
else
    print_warning "cargo-audit not installed, skipping security audit"
    echo "Install with: cargo install cargo-audit"
fi

echo ""
echo -e "${GREEN}🎉 All checks passed! Ready to push to main.${NC}"
echo ""
echo "To push your changes:"
echo "  git add ."
echo "  git commit -m 'Your commit message'"
echo "  git push origin main"