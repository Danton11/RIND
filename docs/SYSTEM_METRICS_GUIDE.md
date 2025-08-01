# RIND System Metrics Guide

This guide covers the RIND system metrics monitoring setup, including the custom system metrics exporter and Grafana dashboard.

## Overview

The RIND system metrics monitoring provides visibility into:

- **Process Metrics**: CPU usage, memory consumption, and uptime for RIND processes
- **Docker Metrics**: Container status and health monitoring
- **Network Metrics**: DNS connection tracking and port-specific monitoring
- **System Resources**: File descriptor usage and system-level metrics
- **Application Metrics**: DNS records count, file sizes, and custom application data

## Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│ System Metrics  │───▶│    Prometheus    │───▶│     Grafana     │
│    Exporter     │    │                  │    │   Dashboard     │
│   (Port 8091)   │    │   (Port 9090)    │    │  (Port 3000)    │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │
         ▼
┌─────────────────┐
│ RIND DNS Server │
│   Processes     │
│ Docker Containers│
│ System Resources│
└─────────────────┘
```

## Components

### 1. System Metrics Exporter

**Location**: `scripts/system-metrics-exporter.py`
**Port**: 8091
**Endpoints**:
- `/metrics` - Prometheus-format metrics
- `/health` - Health check endpoint

**Key Features**:
- Process monitoring for RIND DNS server processes
- Docker container status tracking
- Network connection monitoring for DNS ports
- File descriptor usage tracking
- DNS records file monitoring
- Custom application metrics

### 2. Grafana Dashboard

**Location**: `monitoring/grafana/dashboards/rind-system-metrics.json`
**Dashboard UID**: `rind-system-metrics`
**Title**: "RIND System Metrics"

**Panels**:
1. **RIND Process CPU Usage** - CPU utilization by process
2. **RIND Process Memory Usage** - RSS and VMS memory consumption
3. **Docker Containers** - Total container count
4. **Container Status** - Individual container health status
5. **DNS Connections** - Total DNS connections
6. **DNS Records Count** - Number of DNS records
7. **DNS Connections by Port** - Port-specific connection tracking
8. **System File Descriptors** - Allocated vs maximum file descriptors
9. **File Descriptor Usage** - Usage percentage gauge
10. **DNS Records File Size** - Size of the DNS records file
11. **Process Uptime** - Process and exporter uptime tracking

## Metrics Reference

### Process Metrics

| Metric Name | Type | Description | Labels |
|-------------|------|-------------|---------|
| `rind_process_cpu_percent` | Gauge | CPU usage percentage | `pid`, `name` |
| `rind_process_memory_rss_bytes` | Gauge | Resident Set Size memory | `pid`, `name` |
| `rind_process_memory_vms_bytes` | Gauge | Virtual Memory Size | `pid`, `name` |
| `rind_process_uptime_seconds` | Gauge | Process uptime in seconds | `pid`, `name` |

### Docker Metrics

| Metric Name | Type | Description | Labels |
|-------------|------|-------------|---------|
| `rind_docker_containers_total` | Gauge | Total RIND containers | - |
| `rind_docker_container_up` | Gauge | Container status (1=up, 0=down) | `name` |

### Network Metrics

| Metric Name | Type | Description | Labels |
|-------------|------|-------------|---------|
| `rind_dns_connections_total` | Gauge | Total DNS connections | - |
| `rind_dns_connections_by_port` | Gauge | Connections by port | `port` |

### System Metrics

| Metric Name | Type | Description | Labels |
|-------------|------|-------------|---------|
| `rind_system_file_descriptors_allocated` | Gauge | Allocated file descriptors | - |
| `rind_system_file_descriptors_max` | Gauge | Maximum file descriptors | - |
| `rind_system_file_descriptors_usage_percent` | Gauge | FD usage percentage | - |

### Application Metrics

| Metric Name | Type | Description | Labels |
|-------------|------|-------------|---------|
| `rind_dns_records_count` | Gauge | Number of DNS records | - |
| `rind_dns_records_file_size_bytes` | Gauge | DNS records file size | - |
| `rind_dns_records_file_modified_timestamp` | Gauge | Last modification time | - |
| `rind_metrics_exporter_uptime_seconds` | Gauge | Exporter uptime | - |

## Setup and Configuration

### 1. Docker Compose Setup

The system metrics exporter is included in the full stack Docker Compose configuration:

```yaml
system-metrics-exporter:
  build:
    context: ..
    dockerfile: docker/Dockerfile.system-metrics
  container_name: rind-system-metrics
  ports:
    - "8091:8091/tcp"
  environment:
    - METRICS_PORT=8091
    - METRICS_HOST=0.0.0.0
  volumes:
    - /proc:/host/proc:ro
    - /sys:/host/sys:ro
    - /var/run/docker.sock:/var/run/docker.sock:ro
    - ../dns_records.txt:/app/dns_records.txt:ro
```

### 2. Prometheus Configuration

The system metrics exporter is configured as a scrape target in Prometheus:

```yaml
- job_name: 'rind-system-metrics'
  static_configs:
    - targets: ['system-metrics-exporter:8091']
  scrape_interval: 10s
  metrics_path: /metrics
```

### 3. Grafana Dashboard Import

The dashboard is automatically provisioned when using the full stack setup. To manually import:

1. Open Grafana at http://localhost:3000
2. Login with admin/rind-admin-2025
3. Go to Dashboards → Import
4. Upload `monitoring/grafana/dashboards/rind-system-metrics.json`

## Usage

### Starting the System

```bash
# Start the full stack (includes system metrics)
./scripts/start-fullstack.sh start

# Start with production profile
./scripts/start-fullstack.sh start --production
```

### Testing the Setup

```bash
# Run comprehensive system metrics tests
./scripts/test-system-metrics.sh

# Test specific components
./scripts/test-system-metrics.sh health      # Health checks only
./scripts/test-system-metrics.sh metrics    # Metrics content only
./scripts/test-system-metrics.sh prometheus # Prometheus integration
./scripts/test-system-metrics.sh grafana    # Dashboard availability

# Show current metrics summary
./scripts/test-system-metrics.sh summary
```

### Manual Testing

```bash
# Check system metrics exporter health
curl http://localhost:8091/health

# View raw metrics
curl http://localhost:8091/metrics

# Test Prometheus scraping
curl http://localhost:9090/api/v1/targets | jq '.data.activeTargets[] | select(.labels.job == "rind-system-metrics")'

# Query specific metrics
curl "http://localhost:9090/api/v1/query?query=rind_process_cpu_percent"
```

## Dashboard Features

### Real-time Monitoring

- **Auto-refresh**: 5-second refresh interval
- **Time Range**: Last 15 minutes by default
- **Live Updates**: All panels update automatically

### Visual Indicators

- **Color Coding**: 
  - Green: Healthy/Normal
  - Yellow: Warning thresholds
  - Red: Critical thresholds
- **Gauges**: File descriptor usage with threshold indicators
- **Time Series**: Historical trends for all metrics
- **Status Panels**: Container and connection status

### Alerting Thresholds

- **CPU Usage**: Warning at 70%, Critical at 90%
- **Memory Usage**: Warning at 80%, Critical at 95%
- **File Descriptors**: Warning at 70%, Critical at 90%
- **Container Status**: Alert when containers are down

## Troubleshooting

### Common Issues

1. **Metrics Exporter Not Starting**
   ```bash
   # Check container logs
   docker logs rind-system-metrics
   
   # Verify Python dependencies
   docker exec rind-system-metrics python3 -c "import psutil; print('OK')"
   ```

2. **Missing Metrics in Prometheus**
   ```bash
   # Check Prometheus targets
   curl http://localhost:9090/api/v1/targets
   
   # Verify scrape configuration
   curl http://localhost:9090/api/v1/status/config
   ```

3. **Dashboard Not Loading**
   ```bash
   # Check Grafana logs
   docker logs rind-grafana
   
   # Verify dashboard file
   ls -la monitoring/grafana/dashboards/rind-system-metrics.json
   ```

4. **No Process Metrics**
   ```bash
   # Check if RIND processes are running
   ps aux | grep rind
   
   # Verify Docker socket access
   docker exec rind-system-metrics ls -la /var/run/docker.sock
   ```

### Performance Considerations

- **Scrape Interval**: 10 seconds (configurable)
- **Resource Usage**: ~64MB RAM, minimal CPU
- **Data Retention**: 30 days in Prometheus
- **Network Impact**: ~1KB per scrape

### Security Notes

- System metrics exporter runs as non-root user
- Read-only access to system files
- Docker socket access required for container metrics
- No sensitive data exposed in metrics

## Integration with Other Dashboards

The system metrics dashboard complements other RIND monitoring dashboards:

- **DNS Overview Dashboard**: Application-level metrics
- **DNS Canary Dashboard**: External monitoring metrics
- **System Metrics Dashboard**: Infrastructure-level metrics

All dashboards share the same Prometheus data source and can be viewed together for comprehensive monitoring.

## Customization

### Adding Custom Metrics

1. Edit `scripts/system-metrics-exporter.py`
2. Add new metric collection in `get_custom_metrics()`
3. Rebuild the Docker image
4. Update the Grafana dashboard to include new panels

### Modifying Thresholds

1. Edit dashboard JSON file
2. Update threshold values in panel configurations
3. Re-import dashboard or restart Grafana

### Changing Refresh Rates

1. Update `scrape_interval` in Prometheus configuration
2. Modify dashboard refresh rate in Grafana
3. Adjust exporter collection frequency if needed

## Monitoring Best Practices

1. **Regular Health Checks**: Use the test script regularly
2. **Threshold Tuning**: Adjust based on your environment
3. **Data Retention**: Configure appropriate retention policies
4. **Alerting**: Set up alerts for critical metrics
5. **Documentation**: Keep metrics documentation updated
6. **Performance**: Monitor the monitoring system itself

## Support and Maintenance

- **Log Location**: Container logs via `docker logs`
- **Configuration**: Environment variables and volume mounts
- **Updates**: Rebuild Docker image for script changes
- **Backup**: Include dashboard JSON in backups
- **Monitoring**: Monitor the metrics exporter itself