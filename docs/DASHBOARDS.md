# RIND DNS Server - Dashboards

Access Grafana at: **http://localhost:3000** (admin/rind-admin-2025)

## Available Dashboards

### Core Monitoring
- **DNS Overview** - Main DNS server metrics and performance
- **DNS System Metrics** - Infrastructure and system-level monitoring
- **DNS Canary** - External monitoring and health checks

### Detailed Analysis
- **DNS Protocol** - Protocol-level statistics and analysis
- **DNS Record Management** - Record operations and API performance
- **DNS Infrastructure** - Infrastructure monitoring and capacity
- **DNS Errors** - Error tracking and troubleshooting

## Quick Links

- DNS Overview: http://localhost:3000/d/dns-overview
- System Metrics: http://localhost:3000/d/rind-system-metrics
- Canary Dashboard: http://localhost:3000/d/rind-canary-dashboard

## Metrics Sources

- **DNS Server Metrics**: Port 9090 (primary), 9091 (secondary)
- **System Metrics**: Port 8091
- **Canary Metrics**: Port 8090
- **Prometheus**: Port 9090