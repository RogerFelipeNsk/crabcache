#!/bin/bash

# Security scanning script for CrabCache
# This script runs various security scans on the Docker image

set -e

IMAGE_NAME="crabcache:latest"
SCAN_RESULTS_DIR="./security-reports"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}🔒 CrabCache Security Scanner${NC}"
echo "=================================="

# Create results directory
mkdir -p "$SCAN_RESULTS_DIR"

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to run Docker Scout if available
run_docker_scout() {
    if command_exists docker && docker scout --help >/dev/null 2>&1; then
        echo -e "${YELLOW}📊 Running Docker Scout scan...${NC}"
        docker scout cves "$IMAGE_NAME" --format json > "$SCAN_RESULTS_DIR/docker-scout.json" 2>/dev/null || {
            echo -e "${RED}❌ Docker Scout scan failed${NC}"
            return 1
        }
        echo -e "${GREEN}✅ Docker Scout scan completed${NC}"
        return 0
    else
        echo -e "${YELLOW}⚠️  Docker Scout not available${NC}"
        return 1
    fi
}

# Function to run Trivy scan
run_trivy() {
    if command_exists trivy; then
        echo -e "${YELLOW}📊 Running Trivy scan...${NC}"
        trivy image --format json --output "$SCAN_RESULTS_DIR/trivy.json" "$IMAGE_NAME" || {
            echo -e "${RED}❌ Trivy scan failed${NC}"
            return 1
        }
        echo -e "${GREEN}✅ Trivy scan completed${NC}"
        return 0
    else
        echo -e "${YELLOW}⚠️  Trivy not installed. Install with: curl -sfL https://raw.githubusercontent.com/aquasecurity/trivy/main/contrib/install.sh | sh -s -- -b /usr/local/bin${NC}"
        return 1
    fi
}

# Function to run Grype scan
run_grype() {
    if command_exists grype; then
        echo -e "${YELLOW}📊 Running Grype scan...${NC}"
        grype "$IMAGE_NAME" -o json > "$SCAN_RESULTS_DIR/grype.json" || {
            echo -e "${RED}❌ Grype scan failed${NC}"
            return 1
        }
        echo -e "${GREEN}✅ Grype scan completed${NC}"
        return 0
    else
        echo -e "${YELLOW}⚠️  Grype not installed. Install with: curl -sSfL https://raw.githubusercontent.com/anchore/grype/main/install.sh | sh -s -- -b /usr/local/bin${NC}"
        return 1
    fi
}

# Function to check if image exists
check_image() {
    if ! docker image inspect "$IMAGE_NAME" >/dev/null 2>&1; then
        echo -e "${RED}❌ Image $IMAGE_NAME not found. Build it first with: docker build -t $IMAGE_NAME .${NC}"
        exit 1
    fi
}

# Function to generate summary report
generate_summary() {
    echo -e "${YELLOW}📋 Generating summary report...${NC}"
    
    cat > "$SCAN_RESULTS_DIR/summary.md" << EOF
# Security Scan Summary

**Image**: $IMAGE_NAME  
**Scan Date**: $(date)  
**Scan Results Directory**: $SCAN_RESULTS_DIR

## Scan Results

EOF

    # Check which scans were successful
    if [ -f "$SCAN_RESULTS_DIR/docker-scout.json" ]; then
        echo "- ✅ Docker Scout: Results available in docker-scout.json" >> "$SCAN_RESULTS_DIR/summary.md"
    else
        echo "- ❌ Docker Scout: Scan failed or not available" >> "$SCAN_RESULTS_DIR/summary.md"
    fi

    if [ -f "$SCAN_RESULTS_DIR/trivy.json" ]; then
        echo "- ✅ Trivy: Results available in trivy.json" >> "$SCAN_RESULTS_DIR/summary.md"
    else
        echo "- ❌ Trivy: Scan failed or not available" >> "$SCAN_RESULTS_DIR/summary.md"
    fi

    if [ -f "$SCAN_RESULTS_DIR/grype.json" ]; then
        echo "- ✅ Grype: Results available in grype.json" >> "$SCAN_RESULTS_DIR/summary.md"
    else
        echo "- ❌ Grype: Scan failed or not available" >> "$SCAN_RESULTS_DIR/summary.md"
    fi

    cat >> "$SCAN_RESULTS_DIR/summary.md" << EOF

## Next Steps

1. Review the JSON reports for detailed vulnerability information
2. Update Dockerfile to address any critical vulnerabilities
3. Rebuild the image after applying fixes
4. Re-run this scan to verify fixes

## Security Best Practices

- Regularly update base images
- Keep dependencies up to date
- Use minimal base images when possible
- Run containers as non-root users
- Implement proper network segmentation
- Monitor for security updates

EOF

    echo -e "${GREEN}✅ Summary report generated: $SCAN_RESULTS_DIR/summary.md${NC}"
}

# Main execution
main() {
    echo "Checking if Docker image exists..."
    check_image

    echo "Starting security scans..."
    
    # Run available scanners
    run_docker_scout || true
    run_trivy || true
    run_grype || true

    # Generate summary
    generate_summary

    echo ""
    echo -e "${GREEN}🎉 Security scan completed!${NC}"
    echo -e "Results saved in: ${YELLOW}$SCAN_RESULTS_DIR${NC}"
    echo ""
    echo "To view the summary:"
    echo "  cat $SCAN_RESULTS_DIR/summary.md"
    echo ""
    echo "To install missing scanners:"
    echo "  Trivy: curl -sfL https://raw.githubusercontent.com/aquasecurity/trivy/main/contrib/install.sh | sh -s -- -b /usr/local/bin"
    echo "  Grype: curl -sSfL https://raw.githubusercontent.com/anchore/grype/main/install.sh | sh -s -- -b /usr/local/bin"
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -i|--image)
            IMAGE_NAME="$2"
            shift 2
            ;;
        -o|--output)
            SCAN_RESULTS_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  -i, --image IMAGE    Docker image to scan (default: crabcache:latest)"
            echo "  -o, --output DIR     Output directory for results (default: ./security-reports)"
            echo "  -h, --help          Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0                                    # Scan default image"
            echo "  $0 -i myapp:v1.0                    # Scan specific image"
            echo "  $0 -i myapp:v1.0 -o /tmp/scans      # Custom output directory"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use -h or --help for usage information"
            exit 1
            ;;
    esac
done

# Run main function
main