# DNS Server Monitoring Stack

This document describes the monitoring infrastructure for the RIND DNS server project.

## Overview

The monitoring stack includes:
- **Prometheus**: Metrics collection and storage
- **Grafana**: Visualization and dashboards  
- **Loki**: Log aggregation
- **Promtail**: Log collection agent
- **DNS Servers**: Multiple instances with metrics and logging

## Quick Start

1. Start the monitoring stack:
```bash
docker-compose -f docker-compose.monitoring.yml up -d
```

2. Access the services:
- **Grafana**: http://localhost:3000 (admin/admin)
- **Prometheus**: http://localhost:9090
- **DNS Server 1**: UDP 12312, API 8080, Metrics 9090
- **DNS Server 2**: UDP 12313, API 8081, Metrics 9091

## Architecture

```
DNS Servers (Port 9090/9091) → Prometheus (Port 9090) → Grafana (Port 3000)
DNS Servers (Logs) → Promtail → Loki (Port 3100) → Grafana
```

## Service Discovery

Prometheus automatically discovers DNS server instances using Docker labels:
- `prometheus.io/scrape=true`
- `prometheus.io/port=9090`
- `prometheus.io/path=/metrics`
- `com.docker.compose.service=dns-server`

## Persistent Storage

The following volumes are created for data persistence:
- `prometheus-data`: Metrics storage (15 day retention)
- `grafana-data`: Dashboard and configuration storage
- `loki-data`: Log storage (7 day retention)
- `dns-logs`: Shared log directory for DNS servers

## Configuration Files

- `prometheus/prometheus.yml`: Prometheus scraping configuration
- `loki/loki-config.yml`: Loki storage and retention settings
- `promtail/promtail-config.yml`: Log collection configuration
- `grafana/provisioning/`: Auto-provisioned data sources and dashboards

## Environment Variables

DNS servers support the following monitoring-related environment variables:
- `METRICS_PORT`: Metrics server port (default: 9090)
- `SERVER_ID`: Unique server identifier
- `INSTANCE_ID`: Instance identifier for multi-container deployments
- `LOG_LEVEL`: Logging level (info, debug, error)
- `LOG_FORMAT`: Log format (json for production, text for development)

## Scaling

To add more DNS server instances:

1. Add a new service to `docker-compose.monitoring.yml`
2. Use unique ports for DNS, API, and metrics
3. Set unique `SERVER_ID` and `INSTANCE_ID`
4. Add appropriate Docker labels for service discovery

Example:
```yaml
dns-server-3:
  build: .
  container_name: dns-server-3
  ports:
    - "12314:12312/udp"
    - "8082:8080"
    - "9092:9090"
  environment:
    - SERVER_ID=dns-server-3
    - INSTANCE_ID=dns-server-3
  labels:
    - "prometheus.io/scrape=true"
    - "prometheus.io/port=9090"
    - "com.docker.compose.service=dns-server"
```

## Troubleshooting

1. **Prometheus not discovering services**: Check Docker labels and socket permissions
2. **Grafana data source issues**: Verify service networking and URLs
3. **Missing logs**: Check Promtail configuration and volume mounts
4. **High resource usage**: Adjust retention periods and scrape intervals

## Monitoring Network

All services communicate over the `monitoring-network` bridge network for isolation and service discovery.