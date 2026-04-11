# System Metrics

A Python-based exporter that collects process, Docker, network, and system metrics for RIND DNS servers.

## Architecture

```
System Metrics Exporter (port 8091) --> Prometheus --> Grafana
        |
        v
  RIND processes, Docker containers, /proc, /sys
```

## Metrics

### Process Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `rind_process_cpu_percent` | Gauge | `pid`, `name` | CPU usage |
| `rind_process_memory_rss_bytes` | Gauge | `pid`, `name` | RSS memory |
| `rind_process_memory_vms_bytes` | Gauge | `pid`, `name` | Virtual memory |
| `rind_process_uptime_seconds` | Gauge | `pid`, `name` | Process uptime |

### Docker Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `rind_docker_containers_total` | Gauge | — | Total RIND containers |
| `rind_docker_container_up` | Gauge | `name` | Container status (1=up) |

### Network Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `rind_dns_connections_total` | Gauge | — | Total DNS connections |
| `rind_dns_connections_by_port` | Gauge | `port` | Connections per port |

### System Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `rind_system_file_descriptors_allocated` | Gauge | Allocated FDs |
| `rind_system_file_descriptors_max` | Gauge | Maximum FDs |
| `rind_system_file_descriptors_usage_percent` | Gauge | FD usage % |

### Application Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `rind_dns_records_count` | Gauge | Number of DNS records |
| `rind_dns_records_file_size_bytes` | Gauge | Records file size |

## Setup

Included in the full stack Docker Compose. Runs automatically with `./scripts/start-fullstack.sh start`.

Endpoints:
- `/metrics` — Prometheus-format metrics
- `/health` — health check

## Grafana Dashboard

Dashboard UID: `rind-system-metrics`. Auto-provisioned with the full stack.

Panels: process CPU/memory, container status, DNS connections, file descriptors, records count, uptime.

## Testing

```bash
curl http://localhost:8091/health
curl http://localhost:8091/metrics

# Via test script
./scripts/test-system-metrics.sh
./scripts/test-system-metrics.sh summary
```

## Troubleshooting

```bash
docker logs rind-system-metrics
curl http://localhost:9090/api/v1/targets  # check Prometheus scraping
```
