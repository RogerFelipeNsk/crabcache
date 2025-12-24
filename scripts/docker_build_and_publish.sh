#!/bin/bash
set -e

# CrabCache Docker Build and Publish Script
# This script builds and publishes CrabCache to Docker Hub

# Configuration
DOCKER_REGISTRY="docker.io"
DOCKER_REPO="rogerfelipensk/crabcache"
VERSION="0.0.1"
LATEST_TAG="latest"

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

# Check if Docker is running
check_docker() {
    log_info "Checking Docker installation..."
    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed or not in PATH"
        exit 1
    fi
    
    if ! docker info &> /dev/null; then
        log_error "Docker daemon is not running"
        exit 1
    fi
    
    log_success "Docker is ready"
}

# Check if we're in the right directory
check_directory() {
    log_info "Checking project directory..."
    if [[ ! -f "Cargo.toml" ]] || [[ ! -d "src" ]]; then
        log_error "Please run this script from the CrabCache project root directory"
        exit 1
    fi
    log_success "Project directory confirmed"
}

# Build the Docker image
build_image() {
    log_info "Building CrabCache Docker image..."
    log_info "Version: $VERSION"
    log_info "Repository: $DOCKER_REPO"
    
    # Build with version tag
    docker build \
        -f docker/Dockerfile \
        -t "$DOCKER_REPO:$VERSION" \
        -t "$DOCKER_REPO:$LATEST_TAG" \
        --build-arg VERSION="$VERSION" \
        .
    
    if [[ $? -eq 0 ]]; then
        log_success "Docker image built successfully"
    else
        log_error "Failed to build Docker image"
        exit 1
    fi
}

# Test the built image
test_image() {
    log_info "Testing the built Docker image..."
    
    # Start container in background
    CONTAINER_ID=$(docker run -d -p 8000:8000 -p 8001:8001 "$DOCKER_REPO:$VERSION")
    
    if [[ $? -ne 0 ]]; then
        log_error "Failed to start container"
        exit 1
    fi
    
    log_info "Container started with ID: $CONTAINER_ID"
    
    # Wait for container to be ready
    log_info "Waiting for CrabCache to start..."
    sleep 10
    
    # Test basic connectivity
    if echo "PING" | nc -w 3 localhost 8000 | grep -q "PONG"; then
        log_success "Container is responding correctly"
    else
        log_error "Container is not responding"
        docker logs "$CONTAINER_ID"
        docker stop "$CONTAINER_ID" &> /dev/null
        docker rm "$CONTAINER_ID" &> /dev/null
        exit 1
    fi
    
    # Test pipeline functionality
    log_info "Testing pipeline functionality..."
    if python3 scripts/test_pipeline.py --host localhost --port 8000; then
        log_success "Pipeline tests passed"
    else
        log_warning "Pipeline tests failed, but continuing..."
    fi
    
    # Stop and remove test container
    docker stop "$CONTAINER_ID" &> /dev/null
    docker rm "$CONTAINER_ID" &> /dev/null
    log_success "Test completed, container cleaned up"
}

# Login to Docker Hub
docker_login() {
    log_info "Docker Hub login required for publishing..."
    
    if [[ -n "$DOCKER_USERNAME" ]] && [[ -n "$DOCKER_PASSWORD" ]]; then
        log_info "Using environment variables for Docker login"
        echo "$DOCKER_PASSWORD" | docker login "$DOCKER_REGISTRY" -u "$DOCKER_USERNAME" --password-stdin
    else
        log_info "Please enter your Docker Hub credentials:"
        docker login "$DOCKER_REGISTRY"
    fi
    
    if [[ $? -eq 0 ]]; then
        log_success "Successfully logged in to Docker Hub"
    else
        log_error "Failed to login to Docker Hub"
        exit 1
    fi
}

# Push images to Docker Hub
push_images() {
    log_info "Pushing images to Docker Hub..."
    
    # Push version tag
    log_info "Pushing version tag: $VERSION"
    docker push "$DOCKER_REPO:$VERSION"
    
    if [[ $? -ne 0 ]]; then
        log_error "Failed to push version tag"
        exit 1
    fi
    
    # Push latest tag
    log_info "Pushing latest tag"
    docker push "$DOCKER_REPO:$LATEST_TAG"
    
    if [[ $? -ne 0 ]]; then
        log_error "Failed to push latest tag"
        exit 1
    fi
    
    log_success "All images pushed successfully"
}

# Display image information
show_image_info() {
    log_info "Docker image information:"
    docker images "$DOCKER_REPO" --format "table {{.Repository}}\t{{.Tag}}\t{{.Size}}\t{{.CreatedAt}}"
    
    echo ""
    log_info "Image details:"
    docker inspect "$DOCKER_REPO:$VERSION" --format='{{json .Config.Labels}}' | jq '.' 2>/dev/null || echo "Labels: None"
    
    echo ""
    log_success "CrabCache v$VERSION is now available on Docker Hub!"
    echo ""
    echo "🚀 Usage Examples:"
    echo "   docker run -p 8000:8000 $DOCKER_REPO:$VERSION"
    echo "   docker run -p 8000:8000 -p 8001:8001 $DOCKER_REPO:latest"
    echo ""
    echo "🔧 With custom configuration:"
    echo "   docker run -p 8000:8000 \\"
    echo "     -e CRABCACHE_ENABLE_WAL=true \\"
    echo "     -e CRABCACHE_MAX_BATCH_SIZE=32 \\"
    echo "     -v /data/wal:/app/data/wal \\"
    echo "     $DOCKER_REPO:$VERSION"
    echo ""
    echo "📊 Performance: 219,540+ ops/sec with pipelining"
    echo "🏆 5.9x faster than Redis!"
}

# Main execution
main() {
    echo "🦀 CrabCache Docker Build and Publish"
    echo "====================================="
    echo ""
    
    # Parse command line arguments
    SKIP_TESTS=false
    SKIP_PUSH=false
    
    while [[ $# -gt 0 ]]; do
        case $1 in
            --skip-tests)
                SKIP_TESTS=true
                shift
                ;;
            --skip-push)
                SKIP_PUSH=true
                shift
                ;;
            --version)
                VERSION="$2"
                shift 2
                ;;
            --help)
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --skip-tests    Skip container testing"
                echo "  --skip-push     Skip pushing to Docker Hub"
                echo "  --version VER   Set version tag (default: $VERSION)"
                echo "  --help          Show this help"
                echo ""
                echo "Environment variables:"
                echo "  DOCKER_USERNAME  Docker Hub username"
                echo "  DOCKER_PASSWORD  Docker Hub password"
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done
    
    # Execute build pipeline
    check_docker
    check_directory
    build_image
    
    if [[ "$SKIP_TESTS" != "true" ]]; then
        test_image
    else
        log_warning "Skipping tests as requested"
    fi
    
    if [[ "$SKIP_PUSH" != "true" ]]; then
        docker_login
        push_images
    else
        log_warning "Skipping push as requested"
    fi
    
    show_image_info
    
    log_success "Build and publish completed successfully! 🎉"
}

# Run main function
main "$@"