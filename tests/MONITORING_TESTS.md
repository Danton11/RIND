# Monitoring Integration Tests

This document describes the comprehensive integration tests for the DNS server monitoring stack, covering metrics collection, log aggregation, and visualization components.

## Overview

The monitoring integration tests validate the complete observability pipeline:

1. **Metrics Collection**: DNS servers expose Prometheus metrics
2. **Service Discovery**: Prometheus automatically discovers DNS server instances
3. **Data Visualization**: Grafana dashboards display metrics and logs
4. **Log Aggregation**: Loki collects and stores structured logs
5. **Multi-Instance Support**: Multiple DNS servers are monitored independently

## Test Structure

### Core Test Files

- `tests/monitoring_integration_tests.rs` - Main integration test suite
- `scripts/test-monitoring.sh` - Test orchestration script
- `docker-compose.monitoring.yml` - Monitoring stack configuration

### Test Categories

#### 1. Metrics Exposure Tests (`test_metrics_exposure`)

**Purpose**: Verify DNS servers properly expose Prometheus metrics

**Validation**:
- Metrics endpoint accessibility on configured ports
- Presence of essential DNS metrics:
  - `dns_queries_total` - Query counters by type
  - `dns_responses_total` - Response counters by code
  - `dns_query_duration_seconds` - Latency histograms
- Instance labels for multi-server identification

**Requirements Covered**: 1.1, 1.2, 1.3, 1.5, 1.6, 1.7

#### 2. Prometheus Scraping Tests (`test_prometheus_scraping`)

**Purpose**: Validate Prometheus service discovery and metric collection

**Validation**:
- Target discovery via Docker service discovery
- Healthy target status in Prometheus
- Successful metric queries from Prometheus API
- Proper target labeling and identification

**Requirements Covered**: 2.1, 2.2, 2.3, 2.4

#### 3. End-to-End Monitoring Tests (`test_end_to_end_monitoring`)

**Purpose**: Test complete monitoring pipeline with real DNS traffic

**Process**:
1. Generate DNS traffic (record additions and queries)
2. Wait for metrics collection
3. Verify metrics reflect actual traffic patterns
4. Validate metric accuracy across instances

**Requirements Covered**: 1.1, 1.2, 1.3, 2.1

#### 4. Multi-Instance Monitoring Tests (`test_multi_instance_monitoring`)

**Purpose**: Ensure proper isolation and identification of multiple DNS server instances

**Validation**:
- Distinct metrics per instance
- Proper instance labeling
- Independent traffic patterns
- No metric aggregation between instances

**Requirements Covered**: 1.5, 1.7, 2.4, 3.5

#### 5. Grafana Dashboard Tests (`test_grafana_dashboard_functionality`)

**Purpose**: Verify Grafana integration and dashboard availability

**Validation**:
- Grafana API accessibility
- Dashboard discovery and listing
- DNS-specific dashboard presence
- Data source connectivity

**Requirements Covered**: 3.1, 3.2, 3.3, 3.4, 3.5

#### 6. Log Aggregation Tests (`test_log_aggregation`)

**Purpose**: Test Loki log collection and querying

**Process**:
1. Generate DNS activity to create logs
2. Wait for log collection via Promtail
3. Query logs from Loki API
4. Verify log content and structure

**Requirements Covered**: 5.1, 5.2, 5.3, 5.4

#### 7. Service Discovery Tests (`test_service_discovery`)

**Purpose**: Validate automatic DNS server discovery and labeling

**Validation**:
- Docker service discovery configuration
- Target health monitoring
- Automatic relabeling rules
- Instance identification

**Requirements Covered**: 2.1, 2.2, 2.3, 2.4

#### 8. Full Stack Integration Test (`test_full_monitoring_stack_integration`)

**Purpose**: Comprehensive end-to-end validation of entire monitoring stack

**Process**:
1. Health check all monitoring components
2. Generate comprehensive test traffic
3. Validate complete monitoring pipeline
4. Verify data flow from DNS servers to visualization

**Requirements Covered**: All requirements (1.1-6.4)

## Running the Tests

### Prerequisites

1. Docker and docker-compose installed
2. Rust development environment
3. Network ports available:
   - 9090 (Prometheus)
   - 3000 (Grafana)
   - 3100 (Loki)
   - 12312-12313 (DNS servers)
   - 8080-8081 (API servers)
   - 9092-9093 (Metrics servers)

### Quick Start

```bash
# Run all monitoring tests with automatic stack management
./scripts/test-monitoring.sh full

# Start monitoring stack manually
./scripts/test-monitoring.sh start

# Run specific test
./scripts/test-monitoring.sh test test_metrics_exposure

# Check stack status
./scripts/test-monitoring.sh status

# View logs
./scripts/test-monitoring.sh logs prometheus

# Stop monitoring stack
./scripts/test-monitoring.sh stop
```

### Manual Test Execution

```bash
# Start monitoring stack
docker-compose -f docker-compose.monitoring.yml up -d

# Wait for services to be ready (30-60 seconds)
sleep 60

# Run all monitoring integration tests
cargo test --test monitoring_integration_tests -- --nocapture

# Run specific test
cargo test --test monitoring_integration_tests test_metrics_exposure -- --nocapture
```

## Test Configuration

### Environment Variables

Tests use the following default configuration:

- **Prometheus**: `http://localhost:9090`
- **Grafana**: `http://localhost:3000` (admin/admin)
- **Loki**: `http://localhost:3100`
- **DNS Server 1**: DNS=12312, API=8080, Metrics=9092
- **DNS Server 2**: DNS=12313, API=8081, Metrics=9093

### Test Data

Tests generate synthetic DNS traffic:
- Domain patterns: `test-{instance}-{id}.com`
- IP addresses: `203.0.113.x` range
- Query types: Primarily A records
- Traffic patterns: Varied per instance for isolation testing

## Expected Outcomes

### Success Criteria

1. **Metrics Exposure**: All DNS servers expose metrics on dedicated endpoints
2. **Service Discovery**: Prometheus discovers and scrapes all DNS server instances
3. **Data Collection**: Metrics accurately reflect DNS traffic patterns
4. **Multi-Instance**: Each DNS server instance maintains separate metrics
5. **Visualization**: Grafana can access both metrics and log data
6. **Log Aggregation**: Loki collects and stores DNS server logs
7. **Health Monitoring**: All monitoring components report healthy status

### Performance Expectations

- **Metrics Collection**: <1s latency from DNS activity to metric update
- **Service Discovery**: New instances discovered within 30s
- **Log Collection**: Logs appear in Loki within 30s
- **Query Performance**: Prometheus queries complete within 5s

## Troubleshooting

### Common Issues

1. **Port Conflicts**: Ensure required ports are available
2. **Docker Resources**: Monitoring stack requires ~2GB RAM
3. **Service Startup**: Allow 60s for all services to be ready
4. **Network Connectivity**: Verify Docker networking between containers

### Debug Commands

```bash
# Check container status
docker-compose -f docker-compose.monitoring.yml ps

# View service logs
docker-compose -f docker-compose.monitoring.yml logs prometheus
docker-compose -f docker-compose.monitoring.yml logs grafana
docker-compose -f docker-compose.monitoring.yml logs loki

# Test service endpoints
curl http://localhost:9090/api/v1/status/config  # Prometheus
curl http://localhost:3000/api/health            # Grafana
curl http://localhost:3100/ready                 # Loki
curl http://localhost:9092/metrics               # DNS Server 1
curl http://localhost:9093/metrics               # DNS Server 2
```

### Test Failures

If tests fail:

1. Check monitoring stack health: `./scripts/test-monitoring.sh status`
2. Review service logs: `./scripts/test-monitoring.sh logs`
3. Verify network connectivity between containers
4. Ensure sufficient startup time (60s minimum)
5. Check for port conflicts with other services

## Integration with CI/CD

The monitoring tests can be integrated into CI/CD pipelines:

```bash
# CI-friendly test execution
KEEP_STACK=false ./scripts/test-monitoring.sh full

# Or with explicit cleanup
./scripts/test-monitoring.sh start
cargo test --test monitoring_integration_tests
./scripts/test-monitoring.sh stop
```

## Metrics Validation

The tests validate these key metrics are properly collected:

### DNS Query Metrics
- `dns_queries_total{query_type="A", instance="dns-server-1"}`
- `dns_query_duration_seconds_bucket{query_type="A", le="0.001"}`

### DNS Response Metrics
- `dns_responses_total{response_code="NOERROR", instance="dns-server-1"}`
- `dns_nxdomain_total{instance="dns-server-1"}`
- `dns_servfail_total{instance="dns-server-1"}`

### System Metrics
- `dns_server_uptime_seconds{instance="dns-server-1"}`
- `dns_active_connections{instance="dns-server-1"}`
- `dns_packet_errors_total{instance="dns-server-1"}`

## Log Validation

Tests verify structured logs contain:

- **Timestamp**: ISO 8601 format
- **Level**: info, warn, error, debug
- **Message**: Human-readable description
- **Fields**: Structured data (client_ip, query_type, response_code, etc.)
- **Instance ID**: Container/instance identification

Example log entry:
```json
{
  "timestamp": "2025-01-07T10:30:00.000Z",
  "level": "INFO",
  "message": "DNS query processed",
  "fields": {
    "client_ip": "192.168.1.100",
    "query_type": "A",
    "query_name": "example.com",
    "response_code": "NOERROR",
    "processing_time_ms": 2.5,
    "instance_id": "dns-server-1"
  }
}
```