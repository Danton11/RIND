# RIND - DNS Server in Rust

A high-performance DNS server implementation in Rust with real-time API updates and exceptional throughput capabilities.

## Features

- 🚀 **High Performance**: 18,000+ QPS sustained throughput
- ⚡ **Real-time Updates**: 12ms end-to-end API to DNS resolution
- 🛡️ **Robust**: Handles malformed packets and edge cases gracefully  
- 🔄 **Live Updates**: REST API for dynamic DNS record management
- 📊 **Production Ready**: Comprehensive test suite with stress testing
- 🎯 **Protocol Compliant**: Full DNS protocol implementation
- 📈 **Comprehensive Metrics**: Prometheus-compatible metrics with query tracking, latency measurement, and error monitoring

## Quick Start

### Prerequisites

- **Native**: Rust 1.70+
- **Docker**: Docker Engine 20.10+

### Running the Server

#### Option 1: Native (Development)

```bash
# Clone and build
git clone <repository>
cd RIND
cargo run

# Server will start on:
# - DNS: UDP 127.0.0.1:12312  
# - API: HTTP 127.0.0.1:8080
# - Metrics: HTTP 127.0.0.1:9090/metrics
```

#### Option 2: Docker (Production)

```bash
# Build and run with Docker
docker build -t rind-dns:latest .
docker run -d --name rind-server \
  -p 12312:12312/udp \
  -p 8080:8080/tcp \
  -p 9090:9090/tcp \
  -e DNS_BIND_ADDR="0.0.0.0:12312" \
  -e API_BIND_ADDR="0.0.0.0:8080" \
  -e METRICS_PORT="9090" \
  rind-dns:latest

# Check status
docker logs rind-server
```

### Adding DNS Records

```bash
# Add a DNS record via API
curl -X POST http://127.0.0.1:8080/update \
  -H "Content-Type: application/json" \
  -d '{"name": "example.com", "ip": "93.184.216.34", "ttl": 300, "record_type": "A", "class": "IN", "value": null}'

# Query the record
dig @127.0.0.1 -p 12312 example.com
```

### Monitoring and Metrics

The server exposes comprehensive Prometheus-compatible metrics for monitoring:

```bash
# View all metrics
curl http://127.0.0.1:9090/metrics

# Key metrics include:
# - dns_queries_total{query_type="A",instance="dns-server-123"} - Query counters by type
# - dns_responses_total{response_code="NOERROR",instance="dns-server-123"} - Response counters by code  
# - dns_query_duration_seconds - Query processing latency histogram
# - dns_nxdomain_total - NXDOMAIN response counter
# - dns_servfail_total - SERVFAIL response counter
# - dns_packet_errors_total - Packet parsing error counter
```

**Environment Variables:**
- `METRICS_PORT`: Metrics server port (default: 9090)
- `SERVER_ID`: Server instance identifier for metrics labels

## Performance Metrics

- **End-to-End Time**: ~12ms (API call → DNS resolution)
- **DNS Query Response**: <2ms average
- **API Response Time**: ~10ms average  
- **Maximum Throughput**: 18,000+ QPS
- **Concurrent Operations**: 100% success rate
- **Memory Usage**: ~4MB stable under load

## Project Structure

```
RIND/
├── src/                    # Source code
│   ├── main.rs            # Application entry point
│   ├── server.rs          # DNS server implementation  
│   ├── packet.rs          # DNS packet parsing/building
│   ├── query.rs           # Query handling logic
│   ├── update.rs          # Record management & file I/O
│   └── metrics.rs         # Prometheus metrics & logging
├── tests/                 # Comprehensive test suite
│   ├── unit_packet_tests.rs    # DNS packet unit tests
│   ├── unit_update_tests.rs    # Record management unit tests  
│   └── integration_tests.rs    # End-to-end integration tests
├── benches/               # Performance benchmarks
│   └── dns_benchmarks.rs  # Criterion-based benchmarks
├── src/bin/               # Utility binaries
│   ├── add_records.rs     # Add DNS records utility
│   └── test_runner.rs     # Comprehensive test runner
├── dns_records.txt        # DNS records storage
└── README.md
```

## Testing

### Run All Tests

```bash
# Rust-based comprehensive test suite
cargo run --bin test_runner

# Or run individual test categories
cargo test                           # Unit tests
cargo test --test integration_tests  # Integration tests
cargo bench                         # Performance benchmarks
```

### Individual Tests

```bash
# Unit tests
cargo test --test unit_packet_tests
cargo test --test unit_update_tests

# Integration tests (end-to-end, stress, edge cases)
cargo test --test integration_tests

# Performance benchmarks
cargo bench --bench dns_benchmarks

# Add test records
cargo run --bin add_records
```

### Performance Benchmarks

```bash
# Run detailed performance analysis
cargo bench --bench dns_benchmarks

# Key benchmark results:
# - DNS packet parsing: ~100ns
# - Response building: ~520ns  
# - Record loading (1000 records): ~1.8ms
# - Concurrent parsing (100 packets): ~16µs
```

## API Reference

### Add/Update DNS Record

```http
POST /update
Content-Type: application/json

{
  "name": "example.com",
  "ip": "93.184.216.34", 
  "ttl": 300,
  "record_type": "A",
  "class": "IN",
  "value": null
}
```

## DNS Record Format

Records are stored in `dns_records.txt`:

```
example.com:93.184.216.34:300:A:IN
google.com:8.8.8.8:300:A:IN
localhost:127.0.0.1:86400:A:IN
```

Format: `name:ip:ttl:type:class`

## Docker Deployment

### 🐳 Quick Docker Setup

```bash
# 1. Build the image
docker build -t rind-dns:latest .

# 2. Run the container
docker run -d --name rind-server \
  -p 12312:12312/udp \
  -p 8080:8080/tcp \
  -e DNS_BIND_ADDR="0.0.0.0:12312" \
  -e API_BIND_ADDR="0.0.0.0:8080" \
  -e RUST_LOG=info \
  rind-dns:latest

# 3. Test the deployment
curl -X POST http://127.0.0.1:8080/update \
  -H "Content-Type: application/json" \
  -d '{"name": "docker-test.com", "ip": "192.168.1.100", "ttl": 300, "record_type": "A", "class": "IN", "value": null}'

dig @127.0.0.1 -p 12312 docker-test.com
```

### 📚 Complete Docker Guide

For comprehensive Docker documentation including:
- **Setup & Configuration** - Environment variables, port mapping, volumes
- **Testing & Monitoring** - Health checks, performance testing, debugging
- **Troubleshooting** - Common issues and solutions
- **Cleanup & Teardown** - Complete removal procedures

**👉 See [DOCKER.md](DOCKER.md) for the complete guide**

### 🐳 Docker Compose (Quick Start)

```yaml
# docker-compose.yml
version: '3.8'
services:
  dns-server:
    build: .
    container_name: rind-dns-server
    ports:
      - "12312:12312/udp"
      - "8080:8080/tcp"
    environment:
      - DNS_BIND_ADDR=0.0.0.0:12312
      - API_BIND_ADDR=0.0.0.0:8080
      - RUST_LOG=info
    restart: unless-stopped
```

```bash
# Start with compose
docker-compose up -d

# Run tests against container
cargo run --bin test_runner

# Cleanup
docker-compose down
```

## Architecture

- **Async/Await**: Built on Tokio for high concurrency
- **UDP Server**: Handles DNS queries on port 12312
- **HTTP API**: Warp-based REST API on port 8080  
- **Shared State**: Arc<RwLock> for thread-safe record access
- **File Persistence**: Automatic saving to dns_records.txt

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass: `python3 tests/run_all_tests.py`
5. Submit a pull request

## License

[Add your license here]
