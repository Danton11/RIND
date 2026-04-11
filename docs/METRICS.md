# Metrics and Monitoring

RIND exposes Prometheus-compatible metrics from each DNS server instance. These are scraped by Prometheus and visualized in Grafana.

## Metrics Endpoint

Each server exposes metrics at `http://<host>:<METRICS_PORT>/metrics` (default port 9090).

```bash
curl http://127.0.0.1:9090/metrics
```

## Available Metrics

### DNS Query Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `dns_queries_total` | Counter | `query_type`, `instance` | Total queries by type (A, AAAA, MX, etc.) |
| `dns_query_duration_seconds` | Histogram | `query_type`, `instance` | Query processing latency |
| `dns_queries_per_second` | Gauge | — | Current query rate |

### DNS Response Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `dns_responses_total` | Counter | `response_code`, `instance` | Responses by code (NOERROR, NXDOMAIN, etc.) |
| `dns_nxdomain_total` | Counter | — | NXDOMAIN responses |
| `dns_servfail_total` | Counter | — | SERVFAIL responses |

### System Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `dns_server_uptime_seconds` | Gauge | Server uptime |
| `dns_active_connections` | Gauge | Active DNS connections |
| `dns_packet_errors_total` | Counter | Packet parsing errors |

### Record Management Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `active_records_total` | Gauge | — | Current record count |
| `record_operations_total` | Counter | `operation`, `status` | CRUD operations |
| `record_operation_duration_seconds` | Histogram | `operation` | Operation latency |
| `record_operations_failed_total` | Counter | `operation`, `error_type` | Failed operations |

### API Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `api_requests_total` | Counter | `endpoint`, `method`, `status` | HTTP requests |
| `api_request_duration_seconds` | Histogram | `endpoint`, `method` | API response time |

## Configuration

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `METRICS_PORT` | 9090 | Metrics server port |
| `SERVER_ID` | `dns-server-{PID}` | Instance identifier for labels |

## Grafana Dashboards

Access Grafana at http://localhost:3000 (admin/rind-admin-2025).

### Available Dashboards

- **DNS Overview** — server health, query rates, latency
- **DNS Canary** — external monitoring and health checks
- **DNS System Metrics** — infrastructure-level metrics (CPU, memory, FDs)
- **DNS Protocol** — protocol-level statistics
- **DNS Record Management** — CRUD operations and API performance
- **DNS Infrastructure** — capacity monitoring
- **DNS Errors** — error tracking

Dashboard JSON files are in `monitoring/grafana/dashboards/`.

### Quick Links

- DNS Overview: http://localhost:3000/d/dns-overview
- System Metrics: http://localhost:3000/d/rind-system-metrics
- Canary: http://localhost:3000/d/rind-canary-dashboard

### Metrics Sources

| Source | Port |
|--------|------|
| DNS Server (primary) | 9091 |
| DNS Server (secondary) | 9092 |
| System Metrics Exporter | 8091 |
| Canary | 8090 |
| Prometheus | 9090 |

## Useful PromQL Queries

```promql
# Query rate by type
rate(dns_queries_total[5m])

# Average query latency
rate(dns_query_duration_seconds_sum[5m]) / rate(dns_query_duration_seconds_count[5m])

# P95 latency
histogram_quantile(0.95, rate(dns_query_duration_seconds_bucket[5m]))

# Error rate
rate(dns_packet_errors_total[5m]) + rate(dns_nxdomain_total[5m])

# API requests by endpoint
sum by (endpoint) (api_requests_total)
```

## Prometheus Configuration

```yaml
scrape_configs:
  - job_name: 'rind-dns'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
    metrics_path: /metrics
```

## Troubleshooting

**No metrics showing in Grafana:**
1. Check Prometheus targets: `curl http://localhost:9090/api/v1/targets`
2. Verify metrics endpoint: `curl http://localhost:9091/metrics`
3. Test Prometheus query: `curl 'http://localhost:9090/api/v1/query?query=active_records_total'`
4. Check Grafana datasource points to `http://prometheus:9090`

**Missing specific metrics:**
- Ensure DNS queries are being processed (metrics only appear after first use)
- Check server logs for metrics initialization errors
