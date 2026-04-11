# Grafana Dashboards

Dashboard JSON files for the RIND monitoring stack. Auto-provisioned when using the full stack setup.

## Dashboards

| File | Description |
|------|-------------|
| `dns-overview.json` | Server metrics and performance |
| `dns-canary-dashboard.json` | External monitoring and health checks |
| `rind-system-metrics.json` | Infrastructure metrics (CPU, memory, FDs) |
| `dns-record-management.json` | Record CRUD operations and API performance |
| `dns-protocol.json` | Protocol-level statistics |
| `dns-infrastructure-dashboard.json` | Infrastructure and capacity |
| `dns-errors.json` | Error tracking |

## Manual Import

1. Open Grafana at http://localhost:3000
2. Go to Dashboards > Import
3. Upload the JSON file

## Alert Rules

Configured in `monitoring/prometheus/record-management-alerts.yml`:
- High error rates (>5% for 5 min)
- Slow response times (>100ms p95 for 5 min)
- I/O errors (critical)
- Service availability
