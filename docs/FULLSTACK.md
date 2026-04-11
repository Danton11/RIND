# Full Stack Deployment

The full stack runs two DNS server instances behind HAProxy, with Prometheus, Grafana, Loki, and AlertManager for observability.

## Quick Start

```bash
./scripts/start-fullstack.sh start
```

Services available after startup:

| Service | URL |
|---------|-----|
| DNS (via HAProxy) | `dig @localhost -p 53 example.com` |
| API (via HAProxy) | http://localhost:80/records |
| Grafana | http://localhost:3000 (credentials in `.env`) |
| Prometheus | http://localhost:9090 |
| HAProxy Stats | http://localhost:8404/stats |
| Loki | http://localhost:3100 |
| AlertManager | http://localhost:9093 |

## Architecture

```
Clients
   |
HAProxy (port 53 UDP, port 80 HTTP)
   |
   ├── dns-server-primary   (DNS:12312, API:8080, Metrics:9091)
   └── dns-server-secondary (DNS:12313, API:8081, Metrics:9092)
         |
   Prometheus ──► Grafana
   Loki ────────►
   AlertManager
```

## Services

### DNS Servers

| Service | DNS Port | API Port | Metrics Port |
|---------|----------|----------|-------------|
| dns-server-primary | 12312/udp | 8080/tcp | 9091/tcp |
| dns-server-secondary | 12313/udp | 8081/tcp | 9092/tcp |

Environment variables: `DNS_BIND_ADDR`, `API_BIND_ADDR`, `METRICS_PORT`, `SERVER_ID`, `LOG_FORMAT`

### HAProxy

- Round-robin load balancing for DNS (UDP) and API (HTTP)
- Health checks for backend servers
- Rate limiting (100 req/10s per IP)
- Stats page at port 8404

### Monitoring Stack

**Prometheus** — Scrapes metrics from DNS servers via Docker service discovery. 30-day retention, 10GB limit.

**Grafana** — Pre-configured dashboards and datasources (Prometheus + Loki). See [METRICS.md](METRICS.md) for dashboard details.

**Loki** — Log aggregation from all services. Structured log parsing.

**AlertManager** — Alert routing. Rules configured for DNS down, high error rates, performance degradation.

## Service Profiles

```bash
./scripts/start-fullstack.sh start --production   # HAProxy + AlertManager
./scripts/start-fullstack.sh start --testing       # Testing services
./scripts/start-fullstack.sh start --utilities     # Record manager
./scripts/start-fullstack.sh start --benchmarking  # Benchmarks
```

## Configuration Files

| File | Purpose |
|------|---------|
| `docker/docker-compose.fullstack.yml` | Main orchestration |
| `monitoring/haproxy/haproxy.cfg` | Load balancer config |
| `monitoring/prometheus/prometheus.yml` | Scrape rules |
| `monitoring/alertmanager/config.yml` | Alert routing |
| `monitoring/grafana/provisioning/` | Dashboard/datasource provisioning |
| `monitoring/loki/loki-config.yml` | Log aggregation config |

## Persistent Volumes

- `prometheus-data` — metrics (15-day retention)
- `grafana-data` — dashboards and config
- `loki-data` — logs (7-day retention)
- `dns-logs` — shared log directory

## Testing

```bash
# DNS resolution through HAProxy
dig @localhost -p 53 example.com

# API through HAProxy
curl http://localhost:80/records

# Direct server access
dig @localhost -p 12312 example.com
curl http://localhost:8080/records

# HAProxy stats
curl http://localhost:8404/stats
```

## Troubleshooting

**Services not starting:**
```bash
systemctl status docker
docker system df
./scripts/start-fullstack.sh build  # rebuild images
```

**DNS not resolving:**
```bash
./scripts/start-fullstack.sh logs dns-server-primary
dig @localhost -p 12312 example.com  # bypass HAProxy
curl http://localhost:8404/stats     # check backend health
```

**Monitoring issues:**
```bash
curl http://localhost:9090/api/v1/targets           # Prometheus targets
curl http://admin:$GRAFANA_ADMIN_PASSWORD@localhost:3000/api/datasources  # Grafana
curl http://localhost:3100/ready                     # Loki
```
