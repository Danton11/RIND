# RIND DNS Server - Docker Guide

Complete guide for building, deploying, testing, and managing the RIND DNS server using Docker.

## 📋 Table of Contents

- [Prerequisites](#prerequisites)
- [Building Images](#building-images)
- [Running Containers](#running-containers)
- [Testing](#testing)
- [Monitoring](#monitoring)
- [Troubleshooting](#troubleshooting)
- [Cleanup](#cleanup)

## 🔧 Prerequisites

- Docker Engine 20.10+
- Docker Compose 2.0+ (optional)
- `dig` command for DNS testing
- `curl` for API testing

```bash
# Verify prerequisites
docker --version
docker-compose --version
dig -v
curl --version
```

## 🏗️ Building Images

### Production Image (Recommended)

Includes test utilities and comprehensive tooling:

```bash
# Build production image
docker build -t rind-dns:latest .

# Build with specific tag
docker build -t rind-dns:v1.0.0 .

# Build with build args
docker build \
  --build-arg RUST_VERSION=1.82 \
  -t rind-dns:latest .
```

### Simple Image (Minimal)

Smaller image for production deployments:

```bash
# Build minimal image
docker build -f Dockerfile.simple -t rind-dns:simple .

# Check image sizes
docker images | grep rind-dns
```

### Development Image

For development with hot-reload:

```bash
# Build development image
docker build -f Dockerfile.dev -t rind-dns:dev .
```

## 🚀 Running Containers

### Basic Container

```bash
# Run with default settings
docker run -d --name rind-server \
  -p 12312:12312/udp \
  -p 8080:8080/tcp \
  -p 9090:9090/tcp \
  -e DNS_BIND_ADDR="0.0.0.0:12312" \
  -e API_BIND_ADDR="0.0.0.0:8080" \
  -e METRICS_PORT="9090" \
  rind-dns:latest
```

### Production Container

```bash
# Production deployment with all options
docker run -d --name rind-dns-prod \
  -p 12312:12312/udp \
  -p 8080:8080/tcp \
  -p 9090:9090/tcp \
  -e DNS_BIND_ADDR="0.0.0.0:12312" \
  -e API_BIND_ADDR="0.0.0.0:8080" \
  -e METRICS_PORT="9090" \
  -e RUST_LOG=info \
  --restart unless-stopped \
  --memory="256m" \
  --cpus="1.0" \
  -v dns_data:/app/data \
  --health-cmd="timeout 5s bash -c '</dev/tcp/localhost/8080' || exit 1" \
  --health-interval=30s \
  --health-timeout=10s \
  --health-retries=3 \
  rind-dns:latest
```

### Development Container

```bash
# Development with volume mounts
docker run -d --name rind-dev \
  -p 12312:12312/udp \
  -p 8080:8080/tcp \
  -p 9090:9090/tcp \
  -e DNS_BIND_ADDR="0.0.0.0:12312" \
  -e API_BIND_ADDR="0.0.0.0:8080" \
  -e METRICS_PORT="9090" \
  -e RUST_LOG=debug \
  -v $(pwd):/app \
  -v cargo_cache:/usr/local/cargo/registry \
  rind-dns:dev
```

## 🧪 Testing

### Quick Health Check

```bash
# Check if container is running
docker ps | grep rind

# Check logs
docker logs rind-server

# Test basic connectivity
curl -s http://127.0.0.1:8080/ || echo "API not responding"
dig @127.0.0.1 -p 12312 localhost +short || echo "DNS not responding"
```

### Comprehensive Testing

#### 1. API Testing

```bash
# Add a test record
curl -X POST http://127.0.0.1:8080/update \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test.example.com",
    "ip": "192.168.1.100",
    "ttl": 300,
    "record_type": "A",
    "class": "IN",
    "value": null
  }'

# Verify record was added
dig @127.0.0.1 -p 12312 test.example.com

# Expected output:
# test.example.com.    300    IN    A    192.168.1.100
```

#### 2. DNS Performance Testing

```bash
# Single query timing
time dig @127.0.0.1 -p 12312 google.com

# Concurrent queries
echo "Testing concurrent queries..."
for i in {1..20}; do
  dig @127.0.0.1 -p 12312 google.com +short &
done
wait
echo "Concurrent test completed"

# Sustained load test
echo "Running sustained load test..."
for i in {1..100}; do
  dig @127.0.0.1 -p 12312 localhost +short > /dev/null &
  if (( i % 10 == 0 )); then
    echo "Completed $i queries"
    sleep 0.1
  fi
done
wait
echo "Sustained load test completed"
```

#### 3. End-to-End Testing

```bash
# Test complete workflow
echo "=== End-to-End Test ==="

# 1. Add record via API
echo "1. Adding record..."
curl -s -X POST http://127.0.0.1:8080/update \
  -H "Content-Type: application/json" \
  -d '{
    "name": "e2e-test.com",
    "ip": "203.0.113.42",
    "ttl": 600,
    "record_type": "A",
    "class": "IN",
    "value": null
  }'

# 2. Wait for propagation
echo "2. Waiting for propagation..."
sleep 1

# 3. Query via DNS
echo "3. Querying DNS..."
result=$(dig @127.0.0.1 -p 12312 e2e-test.com +short)

# 4. Verify result
if [ "$result" = "203.0.113.42" ]; then
  echo "✅ End-to-end test PASSED"
else
  echo "❌ End-to-end test FAILED: expected 203.0.113.42, got $result"
fi
```

#### 4. Automated Test Suite

```bash
# Run the comprehensive test suite
# (requires Rust toolchain on host)
cargo run --bin test_runner

# Or run individual test categories
cargo test --test unit_packet_tests
cargo test --test unit_update_tests
cargo test --test integration_tests
cargo bench --bench dns_benchmarks
```

### Load Testing with Docker

```bash
# Create a load testing container
docker run --rm --network host \
  -v $(pwd):/scripts \
  alpine/curl:latest \
  sh -c '
    echo "Starting load test..."
    for i in $(seq 1 50); do
      curl -s -X POST http://127.0.0.1:8080/update \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"load-test-$i.com\", \"ip\": \"192.0.2.$((i % 254 + 1))\", \"ttl\": 300, \"record_type\": \"A\", \"class\": \"IN\", \"value\": null}" &
    done
    wait
    echo "Load test completed"
  '
```

## 📊 Monitoring

### Container Metrics

```bash
# Real-time resource usage
docker stats rind-server

# Container information
docker inspect rind-server

# Process information
docker exec rind-server ps aux 2>/dev/null || echo "ps not available in container"
```

### Application Metrics

```bash
# Prometheus metrics endpoint
curl -s http://127.0.0.1:9090/metrics | head -20

# DNS query metrics
curl -s http://127.0.0.1:9090/metrics | grep dns_queries_total

# Response code metrics
curl -s http://127.0.0.1:9090/metrics | grep dns_responses_total

# Error metrics
curl -s http://127.0.0.1:9090/metrics | grep -E "(dns_nxdomain_total|dns_servfail_total|dns_packet_errors_total)"

# DNS records count
docker exec rind-server wc -l dns_records.txt

# Structured log analysis
docker exec rind-server ls logs/                    # List log files
docker exec rind-server tail -20 logs/rind_YYYY-MM-DD_HH.log  # Recent structured logs

# Connection testing
docker exec rind-server netstat -tulpn 2>/dev/null || echo "netstat not available"
```

### Health Monitoring Script

```bash
#!/bin/bash
# health_check.sh

echo "=== RIND DNS Server Health Check ==="
echo "Timestamp: $(date)"
echo

# Container status
echo "Container Status:"
docker ps --filter name=rind-server --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"
echo

# Resource usage
echo "Resource Usage:"
docker stats rind-server --no-stream --format "table {{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.NetIO}}"
echo

# API health
echo "API Health:"
if curl -s -f http://127.0.0.1:8080/ > /dev/null 2>&1; then
  echo "✅ API is responding"
else
  echo "❌ API is not responding"
fi

# DNS health
echo "DNS Health:"
if dig @127.0.0.1 -p 12312 localhost +short > /dev/null 2>&1; then
  echo "✅ DNS is responding"
else
  echo "❌ DNS is not responding"
fi

# Recent logs
echo
echo "Recent Logs (last 5 lines):"
docker logs rind-server --tail 5
```

### Structured Logging Analysis

The server writes comprehensive structured logs to files inside the container:

```bash
# List available log files
docker exec rind-server ls logs/

# View recent structured logs
docker exec rind-server tail -50 logs/rind_YYYY-MM-DD_HH.log

# Search for specific query types
docker exec rind-server grep 'query_type="A"' logs/rind_YYYY-MM-DD_HH.log

# Find error logs with context
docker exec rind-server grep 'ERROR' logs/rind_YYYY-MM-DD_HH.log

# Monitor logs in real-time (if container is running)
docker exec rind-server tail -f logs/rind_YYYY-MM-DD_HH.log

# Extract performance metrics from logs
docker exec rind-server grep 'processing_time_ms' logs/rind_YYYY-MM-DD_HH.log | tail -10
```

**Example Structured Log Entries:**

Successful DNS Query:
```
2025-07-24T12:48:22.291594Z  INFO ThreadId(07) rind::server: DNS query processed successfully 
client_addr=192.168.65.1:17426 query_id=46562 query_type="A" query_name=example.com 
response_code=0 response_code_str="NOERROR" processing_time_ms=1.2 response_size=65 
instance_id=dns-server-1
```

Error with Debug Context:
```
2025-07-24T12:49:57.979727Z ERROR ThreadId(07) rind::server: Failed to parse DNS packet 
client_addr=192.168.65.1:24587 packet_size=2 error="Packet too short: 2 bytes" 
processing_time_ms=3.0 instance_id=dns-server-1 packet_hex=1234
```

## 🚨 Troubleshooting

### Common Issues

#### Container Won't Start

```bash
# Check for port conflicts
sudo lsof -i :12312
sudo lsof -i :8080

# Check Docker daemon
docker info

# Check image exists
docker images | grep rind-dns

# Run with debug output
docker run --rm -it \
  -p 12312:12312/udp \
  -p 8080:8080/tcp \
  -e DNS_BIND_ADDR="0.0.0.0:12312" \
  -e API_BIND_ADDR="0.0.0.0:8080" \
  -e RUST_LOG=debug \
  rind-dns:latest
```

#### DNS Queries Timeout

```bash
# Check if DNS server is binding to correct interface
docker logs rind-server | grep "DNS server listening"
# Should show: 0.0.0.0:12312 (not 127.0.0.1:12312)

# Test UDP connectivity
nc -u -v 127.0.0.1 12312 < /dev/null

# Check firewall rules
sudo iptables -L | grep 12312
```

#### API Calls Fail

```bash
# Check API server binding
docker logs rind-server | grep "API server listening"
# Should show: 0.0.0.0:8080 (not 127.0.0.1:8080)

# Test TCP connectivity
nc -v 127.0.0.1 8080 < /dev/null

# Test with verbose curl
curl -v http://127.0.0.1:8080/
```

#### Performance Issues

```bash
# Check container resources
docker stats rind-server

# Check system resources
top
free -h
df -h

# Increase container limits
docker update --memory="512m" --cpus="2.0" rind-server
```

### Debug Mode

```bash
# Run container in debug mode
docker run -d --name rind-debug \
  -p 12312:12312/udp \
  -p 8080:8080/tcp \
  -e DNS_BIND_ADDR="0.0.0.0:12312" \
  -e API_BIND_ADDR="0.0.0.0:8080" \
  -e RUST_LOG=debug \
  -e RUST_BACKTRACE=1 \
  rind-dns:latest

# Follow debug logs
docker logs -f rind-debug
```

### Interactive Debugging

```bash
# Access container shell (if available)
docker exec -it rind-server /bin/bash

# Or use a debug container
docker run --rm -it \
  --network container:rind-server \
  --pid container:rind-server \
  alpine:latest \
  sh
```

## 🧹 Cleanup

### Stop Container

```bash
# Graceful stop
docker stop rind-server

# Force stop (if needed)
docker kill rind-server
```

### Remove Container

```bash
# Remove stopped container
docker rm rind-server

# Force remove running container
docker rm -f rind-server
```

### Remove Images

```bash
# Remove specific image
docker rmi rind-dns:latest

# Remove all RIND images
docker images | grep rind-dns | awk '{print $3}' | xargs docker rmi

# Remove dangling images
docker image prune -f
```

### Complete Cleanup

```bash
#!/bin/bash
# cleanup.sh - Complete RIND cleanup script

echo "Stopping RIND containers..."
docker ps -q --filter ancestor=rind-dns | xargs -r docker stop

echo "Removing RIND containers..."
docker ps -aq --filter ancestor=rind-dns | xargs -r docker rm

echo "Removing RIND images..."
docker images -q rind-dns | xargs -r docker rmi

echo "Cleaning up volumes..."
docker volume ls -q | grep dns | xargs -r docker volume rm

echo "Cleaning up networks..."
docker network ls -q | grep dns | xargs -r docker network rm

echo "Cleaning up system..."
docker system prune -f

echo "Cleanup completed!"
```

### Docker Compose Cleanup

```bash
# Stop and remove services
docker-compose down

# Remove volumes and orphaned containers
docker-compose down -v --remove-orphans

# Remove images
docker-compose down --rmi all
```

## 📈 Performance Benchmarks

Expected performance metrics for the containerized DNS server:

| Metric | Expected Value | Test Command |
|--------|----------------|--------------|
| DNS Query Response | <2ms | `time dig @127.0.0.1 -p 12312 localhost` |
| API Response | ~10ms | `time curl -X POST http://127.0.0.1:8080/update -d '{...}'` |
| Concurrent Queries | 1,700+ QPS | Load testing script |
| Memory Usage | ~4MB | `docker stats rind-server` |
| Container Startup | <5s | `time docker run ...` |
| End-to-End | ~75ms | Integration test suite |

## 🔗 Additional Resources

- [Main README](README.md) - Project overview and features
- [Docker Compose Examples](docker-compose.yml) - Production deployment
- [Test Documentation](tests/README.md) - Comprehensive testing guide
- [Dockerfile Reference](Dockerfile) - Build configuration details

## 📞 Support

If you encounter issues:

1. Check the [Troubleshooting](#troubleshooting) section
2. Review container logs: `docker logs rind-server`
3. Verify your Docker setup: `docker info`
4. Test with minimal configuration first
5. Check for port conflicts: `sudo lsof -i :12312 -i :8080`

For performance issues, ensure your system meets the minimum requirements and consider adjusting container resource limits.