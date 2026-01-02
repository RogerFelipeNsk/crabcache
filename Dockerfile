# Multi-stage build for CrabCache
FROM rust:1.92-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy dependency files first for better caching
COPY Cargo.toml build.rs ./
COPY proto ./proto

# Create empty src directory and main.rs for dependency caching
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies first (this layer will be cached)
RUN cargo build --release && rm -rf src

# Copy actual source code
COPY src ./src
COPY config ./config

# Build the application in release mode
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Metadata labels
LABEL maintainer="Roger Felipe <rogerfelipe.nsk@gmail.com>"
LABEL version="0.0.2"
LABEL description="High-performance in-memory cache server written in Rust"

# Install runtime dependencies and update gnupg2 to fix CVE-2025-68973
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    netcat-openbsd \
    && apt-get upgrade -y gnupg2 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/crabcache /usr/local/bin/crabcache

# Create data directory for WAL and logs
RUN mkdir -p /app/data/wal /app/logs

# Expose ports (main server and metrics)
EXPOSE 8000 9090

# Health check
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

# Default command
CMD ["crabcache"]