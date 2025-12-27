# Multi-stage build for CrabCache v0.0.1
FROM rust:1.92-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy dependency files first for better caching
COPY Cargo.toml ./
COPY build.rs ./
COPY proto ./proto
# Note: Cargo.lock will be generated during build if not present

# Create empty src directory and main.rs for dependency caching
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies first (this layer will be cached)
RUN cargo build --release && rm -rf src

# Copy actual source code and proto files
COPY src ./src
COPY proto ./proto
COPY config ./config
COPY examples ./examples
COPY benches ./benches

# Build the application in release mode with optimizations
# Enable protobuf support with stub implementations for Docker
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Metadata labels
LABEL maintainer="Roger Felipe <rogerfelipe.nsk@gmail.com>"
LABEL version="0.0.2"
LABEL description="Ultra-high-performance cache server with configurable eviction strategies - Educational project"
LABEL org.opencontainers.image.title="CrabCache"
LABEL org.opencontainers.image.description="Ultra-high-performance cache server with TinyLFU eviction strategies and pipelining support - Educational project"
LABEL org.opencontainers.image.version="0.0.2"
LABEL org.opencontainers.image.authors="Roger Felipe <rogerfelipe.nsk@gmail.com>"
LABEL org.opencontainers.image.url="https://github.com/RogerFelipeNsk/crabcache"
LABEL org.opencontainers.image.documentation="https://github.com/RogerFelipeNsk/crabcache/blob/main/README.md"
LABEL org.opencontainers.image.source="https://github.com/RogerFelipeNsk/crabcache"
LABEL org.opencontainers.image.licenses="MIT"
LABEL org.opencontainers.image.created="2025-12-24"

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/crabcache /usr/local/bin/crabcache
# Don't copy config files - force environment variable usage

# Create data directory for WAL and logs
RUN mkdir -p /app/data/wal /app/logs

# Expose ports (main server and metrics)
EXPOSE 8000 9090

# Health check using netcat to test server connectivity
HEALTHCHECK --interval=30s --timeout=10s --start-period=20s --retries=3 \
    CMD echo "PING" | nc -w 3 localhost 8000 | grep -q "PONG" || exit 1

# Run as non-root user for security
RUN useradd -r -s /bin/false -d /app crabcache && \
    chown -R crabcache:crabcache /app
USER crabcache

# Set environment variables for production
ENV RUST_LOG=info
ENV CRABCACHE_BIND_ADDR=0.0.0.0
ENV CRABCACHE_PORT=8000
ENV CRABCACHE_ENABLE_PIPELINING=true
ENV CRABCACHE_MAX_BATCH_SIZE=16
ENV CRABCACHE_ENABLE_WAL=false

# Default command - no config file available, will use defaults + env vars
CMD ["crabcache"]