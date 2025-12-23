# Multi-stage build for CrabCache
FROM rust:1.92-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./

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

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/crabcache /usr/local/bin/crabcache
COPY --from=builder /app/config ./config

# Create data directory for WAL
RUN mkdir -p /app/data/wal

# Expose ports
EXPOSE 8000 8001

# Health check (simplified since we don't have metrics endpoint yet)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD pgrep crabcache || exit 1

# Run as non-root user
RUN useradd -r -s /bin/false crabcache && \
    chown -R crabcache:crabcache /app
USER crabcache

CMD ["crabcache"]