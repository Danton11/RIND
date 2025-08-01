# RIND DNS Server - Full Stack Deployment

This document describes the production deployment of RIND DNS Server with monitoring, load balancing, and observability.

## 🚀 Quick Start

```bash
# Start the complete stack
./scripts/start-fullstack.sh start --production

# Check status
./scripts/start-fullstack.sh status

# View logs
./scripts/start-fullstack.sh logs

# Stop everything
./scripts/start-fullstack.sh stop
```

## 📋 Architecture Overview

The full stack includes:

### Core Services
- **DNS Server Cluster**: Primary + Secondary instances with load balancing
- **Load Balancer**: HAProxy for DNS (UDP) and API (HTTP) traffic distribution
- **Monitoring Stack**: Prometheus, Grafana, Loki, AlertManager
- **Testing Suite**: Automated testing and benchmarking tools

### Network Architecture
```
Internet/Clients
       ↓
   HAProxy LB (Port 53 UDP, 80 HTTP)
       ↓
DNS Servers (Primary:12312, Secondary:12313)
       ↓
Monitoring (Prometheus:9090, Grafana:3000)
```

## 🛠 Services Breakdown

### DNS Server Instances

| Service | Ports | Purpose | Health Check |
|---------|-------|---------|--------------|
| dns-server-primary | 12312/udp, 8080/tcp, 9091/tcp | Primary DNS server | HTTP /health |
| dns-server-secondary | 12313/udp, 8081/tcp, 9092/tcp | Secondary DNS server | HTTP /health |

**Environment Variables:**
- `DNS_BIND_ADDR`: DNS server bind address
- `API_BIND_ADDR`: HTTP API bind address  
- `METRICS_PORT`: Prometheus metrics port
- `SERVER_ID`: Unique server identifier
- `LOG_FORMAT`: json (for structured logging)

### Load Balancer (HAProxy)

| Port | Protocol | Purpose |
|------|----------|---------|
| 53 | UDP | DNS load balancing |
| 80 | HTTP | API load balancing |
| 8404 | HTTP | HAProxy statistics |

**Features:**
- Round-robin load balancing
- Health checks for backend servers
- Rate limiting (100 req/10s per IP)
- Security headers
- Prometheus metrics export

### Monitoring Stack

#### Prometheus (Port 9090)
- Metrics collection from DNS servers
- 30-day retention, 10GB storage limit
- Service discovery via Docker labels
- Alert rule evaluation

#### Grafana (Port 3000)
- **Credentials**: admin / rind-admin-2025
- Pre-configured dashboards for DNS metrics
- Unified alerting enabled
- Data sources: Prometheus + Loki

#### Loki (Port 3100)
- Log aggregation from all services
- Structured log parsing
- Integration with Grafana for log visualization

#### AlertManager (Port 9093)
- Alert routing and notification
- Webhook integrations
- Alert grouping and inhibition rules

## 🔧 Configuration Files

### Core Configuration
- `docker/docker-compose.fullstack.yml` - Main orchestration file
- `monitoring/haproxy/haproxy.cfg` - Load balancer configuration
- `monitoring/alertmanager/config.yml` - Alert routing rules

### Monitoring Configuration
- `monitoring/prometheus/prometheus.yml` - Metrics collection rules
- `monitoring/grafana/provisioning/` - Dashboard and datasource provisioning
- `monitoring/loki/loki-config.yml` - Log aggregation configuration

## 🚦 Service Profiles

Use profiles to control which services start:

```bash
# Production deployment (includes HAProxy + AlertManager)
./scripts/start-fullstack.sh start --production

# Include testing services
./scripts/start-fullstack.sh start --testing

# Include utility services (record manager)
./scripts/start-fullstack.sh start --utilities

# Include benchmarking services
./scripts/start-fullstack.sh start --benchmarking
```

## 📊 Monitoring & Observability

### Key Metrics Monitored
- **DNS Performance**: Query latency, throughput, error rates
- **System Resources**: CPU, memory, disk usage
- **Network**: Connection counts, bandwidth utilization
- **Application**: Request rates, response times, error counts

### Dashboards Available
1. **DNS Server Overview** - High-level service health
2. **Performance Metrics** - Latency and throughput analysis
3. **System Resources** - Infrastructure monitoring
4. **Error Analysis** - Error rates and failure patterns

### Alert Rules
- **Critical**: DNS server down, high error rates
- **High**: Performance degradation, resource exhaustion
- **Warning**: Elevated latency, approaching limits

## 🧪 Testing & Validation

### Automated Testing
```bash
# Run comprehensive test suite
./scripts/start-fullstack.sh test

# Run performance benchmarks
./scripts/start-fullstack.sh benchmark

# Check service health
./scripts/start-fullstack.sh health
```

### Manual Testing
```bash
# Test DNS resolution
dig @localhost -p 53 example.com

# Test API endpoints
curl http://localhost:80/records

# Test load balancer
curl http://localhost:8404/stats
```

## 🔄 Scaling & High Availability

### Horizontal Scaling
```bash
# Scale to 3 DNS server instances
./scripts/start-fullstack.sh scale 3
```

### Backup & Recovery
```bash
# Backup configuration and data
./scripts/start-fullstack.sh backup

# Restore from backup
./scripts/start-fullstack.sh restore
```

## 🐛 Troubleshooting

### Common Issues

#### Services Not Starting
```bash
# Check Docker daemon
systemctl status docker

# Check resource usage
docker system df
docker system prune -f

# Rebuild images
./scripts/start-fullstack.sh build
```

#### DNS Resolution Issues
```bash
# Check DNS server logs
./scripts/start-fullstack.sh logs dns-server-primary

# Test direct server connection
dig @localhost -p 12312 example.com

# Check HAProxy backend status
curl http://localhost:8404/stats
```

#### Monitoring Issues
```bash
# Check Prometheus targets
curl http://localhost:9090/api/v1/targets

# Verify Grafana datasources
curl http://admin:rind-admin-2025@localhost:3000/api/datasources

# Check Loki ingestion
curl http://localhost:3100/ready
```

### Log Locations
- **Container logs**: `docker-compose logs <service>`
- **Application logs**: `logs/` directory
- **HAProxy logs**: Via Docker logs
- **System logs**: `/var/log/` on host

## 🔒 Security Considerations

### Network Security
- Services isolated in Docker network
- HAProxy provides rate limiting
- Security headers added to HTTP responses
- No direct external access to internal services

### Access Control
- Grafana authentication required
- HAProxy stats protected
- Prometheus metrics on internal network only
- AlertManager webhook authentication

### Data Protection
- Persistent volumes for data storage
- Regular backup procedures
- Log rotation and retention policies
- Secure credential management

## 📈 Performance Tuning

### Resource Limits
- DNS servers: 128MB RAM, 0.5 CPU cores
- Prometheus: 2GB RAM, 1.0 CPU cores  
- Grafana: 512MB RAM, 0.5 CPU cores
- HAProxy: Minimal resource usage

### Optimization Tips
1. **DNS Servers**: Tune worker thread count based on load
2. **HAProxy**: Adjust connection limits and timeouts
3. **Prometheus**: Configure retention based on storage
4. **Grafana**: Optimize dashboard queries

## 🔄 Maintenance

### Regular Tasks
- Monitor disk usage and clean up old data
- Update Docker images for security patches
- Review and tune alert thresholds
- Backup configuration and data

### Health Checks
- Automated health checks every 15-30 seconds
- Manual health verification via script
- Monitoring dashboard review
- Performance baseline comparison

## 📚 Additional Resources

- [Docker Compose Documentation](../DOCKER.md)
- [Monitoring Setup](../MONITORING.md)
- [Performance Metrics](../METRICS.md)
- [Project Structure](../PROJECT_STRUCTURE.md)

---

For questions or issues, check the troubleshooting section above or review the individual service documentation.