# Multi-stage build for optimal image size
FROM rust:1.82-slim AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifests
COPY Cargo.toml ./

# Copy source code
COPY src ./src
COPY benches ./benches
COPY tests ./tests

# Build the application in release mode
RUN cargo build --release

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

# Copy DNS records file
COPY dns_records.txt ./dns_records.txt

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