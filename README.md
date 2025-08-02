# RIND DNS Server

A high-performance DNS server written in Rust with real-time record management and monitoring capabilities.

## ✨ Features

- 🚀 **High Performance**: 18,000+ QPS sustained throughput
- ⚡ **Real-time Updates**: 12ms end-to-end API to DNS resolution
- 🛡️ **Robust**: Handles malformed packets and edge cases gracefully  
- 📊 **Production Ready**: Full test suite with monitoring and alerting
- 🎯 **Protocol Compliant**: Full DNS protocol implementation
- 📈 **Metrics & Monitoring**: Prometheus-compatible metrics with Grafana dashboards

## 🚀 Quick Start

### Prerequisites
- Docker and Docker Compose
- Python 3.9+ (for canary monitoring)

### Start Complete Stack
```bash
# Start DNS servers with full monitoring stack
./scripts/start-fullstack.sh start

# Access monitoring dashboards
# Grafana: http://localhost:3000 (admin/rind-admin-2025)
# Prometheus: http://localhost:9090
# DNS API: http://localhost:8080
```

### Start Canary Monitoring
```bash
# Start external monitoring
./scripts/start-canary.sh start --daemon

# Check status
./scripts/start-canary.sh status

# Stop monitoring
./scripts/start-canary.sh stop
```

### Test DNS Server
```bash
# Query DNS server
dig @localhost -p 12312 example.com

# Add DNS record via API
curl -X POST http://localhost:8080/records \
  -H "Content-Type: application/json" \
  -d '{"name": "test.example.com", "ip": "192.168.1.100", "ttl": 300}'
```

## 📊 Monitoring Dashboards

Access the monitoring dashboards after starting the full stack:

- **DNS Overview**: Main server metrics and performance
- **DNS Canary**: External monitoring and health checks  
- **DNS System Metrics**: Infrastructure and system-level metrics
- **DNS Protocol**: Protocol-level statistics and analysis
- **DNS Record Management**: Record operations and API performance

## 🔧 API Usage

```bash
# Add a DNS record
curl -X POST http://localhost:8080/records \
  -H "Content-Type: application/json" \
  -d '{"name": "example.com", "ip": "93.184.216.34", "ttl": 300}'

# Query the record
dig @localhost -p 12312 example.com

# List all records
curl http://localhost:8080/records

# Update a record
curl -X PUT http://localhost:8080/records/example.com \
  -H "Content-Type: application/json" \
  -d '{"ip": "192.168.1.200", "ttl": 600}'

# Delete a record
curl -X DELETE http://localhost:8080/records/example.com
```

## 📈 Performance

- **Throughput**: 18,000+ QPS sustained
- **Latency**: <2ms DNS queries, ~12ms API to DNS resolution
- **Memory**: ~4MB stable under load
- **Concurrent Operations**: 100% success rate

## 🔧 Development

### Native Development
```bash
# Prerequisites: Rust 1.70+
cargo build --release
cargo run --bin rind

# Run tests
cargo test

# Run benchmarks
cargo bench
```

## 📚 Documentation

- **[Full Stack Deployment](FULLSTACK.md)** - Complete production setup
- **[Metrics & Monitoring](METRICS.md)** - Detailed metrics documentation  
- **[Docker Guide](DOCKER.md)** - Container deployment
- **[Dashboard Guide](DASHBOARDS.md)** - Monitoring dashboards

## 🏗️ Architecture

- **Async/Await**: Built on Tokio for high concurrency
- **UDP Server**: Handles DNS queries on port 12312
- **HTTP API**: REST API on port 8080  
- **Shared State**: Thread-safe record management
- **File Persistence**: Automatic saving to dns_records.txt
- **Monitoring**: Prometheus metrics with Grafana dashboards

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass: `cargo test`
5. Submit a pull request