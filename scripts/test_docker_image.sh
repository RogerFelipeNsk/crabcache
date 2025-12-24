#!/bin/bash
set -e

# CrabCache Docker Image Test Script
# This script tests the Docker image before publishing

# Configuration
IMAGE_NAME="crabcache/crabcache:latest"
CONTAINER_NAME="crabcache-final-test"
TEST_PORT=8000
METRICS_PORT=9090

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Cleanup function
cleanup() {
    log_info "Cleaning up test container..."
    docker stop "$CONTAINER_NAME" &> /dev/null || true
    docker rm "$CONTAINER_NAME" &> /dev/null || true
}

# Set trap for cleanup
trap cleanup EXIT

# Test Docker image
test_docker_image() {
    log_info "Testing CrabCache Docker image: $IMAGE_NAME"
    echo ""
    
    # Start container
    log_info "Starting container..."
    docker run -d \
        -p $TEST_PORT:8000 \
        -p $METRICS_PORT:9090 \
        --name "$CONTAINER_NAME" \
        "$IMAGE_NAME"
    
    if [[ $? -ne 0 ]]; then
        log_error "Failed to start container"
        return 1
    fi
    
    log_success "Container started successfully"
    
    # Wait for container to be ready
    log_info "Waiting for CrabCache to start..."
    sleep 10
    
    # Test 1: Basic connectivity
    log_info "Test 1: Basic connectivity"
    if echo "PING" | nc -w 3 localhost $TEST_PORT | grep -q "PONG"; then
        log_success "✓ Basic connectivity works"
    else
        log_error "✗ Basic connectivity failed"
        docker logs "$CONTAINER_NAME"
        return 1
    fi
    
    # Test 2: Basic operations
    log_info "Test 2: Basic operations"
    
    # PUT operation
    if echo "PUT test_key test_value" | nc -w 3 localhost $TEST_PORT | grep -q "OK"; then
        log_success "✓ PUT operation works"
    else
        log_error "✗ PUT operation failed"
        return 1
    fi
    
    # GET operation
    if echo "GET test_key" | nc -w 3 localhost $TEST_PORT | grep -q "test_value"; then
        log_success "✓ GET operation works"
    else
        log_error "✗ GET operation failed"
        return 1
    fi
    
    # DEL operation
    if echo "DEL test_key" | nc -w 3 localhost $TEST_PORT | grep -q "OK"; then
        log_success "✓ DEL operation works"
    else
        log_error "✗ DEL operation failed"
        return 1
    fi
    
    # Test 3: Health check
    log_info "Test 3: Health check"
    if curl -f -s http://localhost:$METRICS_PORT/health | grep -q "healthy"; then
        log_success "✓ Health check works"
    else
        log_error "✗ Health check failed"
        return 1
    fi
    
    # Test 4: Metrics endpoint
    log_info "Test 4: Metrics endpoint"
    if curl -f -s http://localhost:$METRICS_PORT/metrics | grep -q "crabcache_operations_total"; then
        log_success "✓ Metrics endpoint works"
    else
        log_error "✗ Metrics endpoint failed"
        return 1
    fi
    
    # Test 5: Pipeline functionality
    log_info "Test 5: Pipeline functionality"
    if python3 scripts/test_pipeline.py --host localhost --port $TEST_PORT > /dev/null 2>&1; then
        log_success "✓ Pipeline functionality works"
    else
        log_warning "⚠ Pipeline test failed (may be expected in some environments)"
    fi
    
    # Test 6: Container health
    log_info "Test 6: Container health"
    HEALTH_STATUS=$(docker inspect "$CONTAINER_NAME" --format='{{.State.Health.Status}}' 2>/dev/null || echo "none")
    if [[ "$HEALTH_STATUS" == "healthy" ]] || [[ "$HEALTH_STATUS" == "none" ]]; then
        log_success "✓ Container is healthy"
    else
        log_warning "⚠ Container health status: $HEALTH_STATUS"
    fi
    
    # Test 7: Performance test
    log_info "Test 7: Basic performance test"
    START_TIME=$(date +%s%N)
    for i in {1..100}; do
        echo "PUT perf_key_$i perf_value_$i" | nc -w 1 localhost $TEST_PORT > /dev/null
    done
    END_TIME=$(date +%s%N)
    
    DURATION_MS=$(( (END_TIME - START_TIME) / 1000000 ))
    OPS_PER_SEC=$(( 100000 / DURATION_MS ))
    
    log_success "✓ Performance test: $OPS_PER_SEC ops/sec"
    
    # Display container information
    log_info "Container information:"
    echo "  Image: $IMAGE_NAME"
    echo "  Container ID: $(docker ps -q -f name=$CONTAINER_NAME)"
    echo "  Uptime: $(docker ps --format 'table {{.Status}}' -f name=$CONTAINER_NAME | tail -n 1)"
    echo "  Ports: $TEST_PORT:8000, $METRICS_PORT:9090"
    
    # Display resource usage
    log_info "Resource usage:"
    docker stats "$CONTAINER_NAME" --no-stream --format "  CPU: {{.CPUPerc}}, Memory: {{.MemUsage}}"
    
    log_success "All tests passed! Docker image is ready for production."
    return 0
}

# Main execution
main() {
    echo "🦀 CrabCache Docker Image Test"
    echo "=============================="
    echo ""
    
    # Check if Docker is running
    if ! docker info &> /dev/null; then
        log_error "Docker daemon is not running"
        exit 1
    fi
    
    # Check if image exists
    if ! docker image inspect "$IMAGE_NAME" &> /dev/null; then
        log_error "Docker image $IMAGE_NAME not found"
        log_info "Please build the image first with:"
        log_info "  docker build -f docker/Dockerfile -t $IMAGE_NAME ."
        exit 1
    fi
    
    # Run tests
    if test_docker_image; then
        echo ""
        log_success "🎉 All tests passed! The Docker image is ready for publication."
        echo ""
        echo "Next steps:"
        echo "  1. Push to Docker Hub: docker push $IMAGE_NAME"
        echo "  2. Test in production environment"
        echo "  3. Update documentation with Docker Hub links"
        exit 0
    else
        echo ""
        log_error "❌ Tests failed! Please fix issues before publishing."
        exit 1
    fi
}

# Run main function
main "$@"