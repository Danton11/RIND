# Multi-stage build for optimal image size
FROM rust:1.88-slim AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifests and lock file
COPY Cargo.toml Cargo.lock ./

# Build dependencies only (cached until Cargo.toml/Cargo.lock change)
RUN mkdir -p src/bin benches tests && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn lib() {}" > src/lib.rs && \
    echo "fn main() {}" > src/bin/add_records.rs && \
    echo "fn main() {}" > src/bin/test_runner.rs && \
    echo "fn main() {}" > benches/dns_benchmarks.rs && \
    cargo build --release && \
    rm -rf src benches

# Copy actual source code
COPY src ./src
COPY benches ./benches
COPY tests ./tests

# Build the application (deps already cached).
# COPY preserves host mtimes, so freshly copied sources are often *older*
# than the dummy binary built above — cargo then thinks nothing changed.
# `touch` forces the real build to run.
RUN find src benches tests -name '*.rs' -exec touch {} + && \
    cargo build --release

# Build utilities
RUN cargo build --release --bin add_records
RUN cargo build --release --bin test_runner

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -r -s /bin/false -m -d /app appuser

# Set working directory
WORKDIR /app

# Copy binaries from builder stage
COPY --from=builder /app/target/release/rind ./rind
COPY --from=builder /app/target/release/add_records ./add_records
COPY --from=builder /app/target/release/test_runner ./test_runner

# Create data directory for DNS records
RUN mkdir -p /app/data

# Copy DNS records file to data directory if it exists
# This will be created by the application if it doesn't exist

# Change ownership to app user
RUN chown -R appuser:appuser /app

# Switch to app user
USER appuser

# Expose ports
EXPOSE 12312/udp 8080/tcp 9090/tcp

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD timeout 5s bash -c '</dev/tcp/localhost/8080' || exit 1

# Default command
CMD ["./rind"]