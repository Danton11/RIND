# Monitoring Integration Tests

Tests for the full observability pipeline: metrics collection, log aggregation, Grafana dashboards, and multi-instance support.

## Test File

`tests/monitoring_integration_tests.rs`

## Running

```bash
# Full automated run (starts/stops monitoring stack)
./scripts/test-monitoring.sh full

# Manual
docker-compose -f docker-compose.monitoring.yml up -d
sleep 60  # wait for services
cargo test --test monitoring_integration_tests -- --nocapture
```

## Tests

| Test | What it validates |
|------|-------------------|
| `test_metrics_exposure` | DNS servers expose Prometheus metrics with correct labels |
| `test_prometheus_scraping` | Prometheus discovers and scrapes DNS targets |
| `test_end_to_end_monitoring` | Generate traffic, verify metrics reflect it |
| `test_multi_instance_monitoring` | Each server has distinct, isolated metrics |
| `test_grafana_dashboard_functionality` | Grafana API accessible, DNS dashboards present |
| `test_log_aggregation` | Loki collects DNS server logs |
| `test_service_discovery` | Docker service discovery and relabeling |
| `test_full_monitoring_stack_integration` | End-to-end: health check, traffic, pipeline verification |

## Required Ports

9090 (Prometheus), 3000 (Grafana), 3100 (Loki), 12312-12313 (DNS), 8080-8081 (API), 9092-9093 (Metrics)

## Troubleshooting

```bash
./scripts/test-monitoring.sh status
./scripts/test-monitoring.sh logs prometheus

curl http://localhost:9090/api/v1/targets
curl http://localhost:3000/api/health
curl http://localhost:3100/ready
```
