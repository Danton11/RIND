# Docker Guide

## Building

```bash
# Production image (multi-stage, slim)
docker build -f docker/Dockerfile -t rind-dns:latest .

# Development image (full toolchain)
docker build -f docker/Dockerfile.dev -t rind-dns:dev .

# Minimal image
docker build -f docker/Dockerfile.simple -t rind-dns:simple .
```

The production Dockerfile uses a dependency caching layer — only source changes trigger recompilation, not dependency downloads.

## Running

```bash
# Basic
docker run -d --name rind-server \
  -p 12312:12312/udp \
  -p 8080:8080/tcp \
  -p 9090:9090/tcp \
  -e DNS_BIND_ADDR="0.0.0.0:12312" \
  -e API_BIND_ADDR="0.0.0.0:8080" \
  -e METRICS_PORT="9090" \
  rind-dns:latest

# Production (with limits, restart policy, volume)
docker run -d --name rind-dns-prod \
  -p 12312:12312/udp \
  -p 8080:8080/tcp \
  -p 9090:9090/tcp \
  -e DNS_BIND_ADDR="0.0.0.0:12312" \
  -e API_BIND_ADDR="0.0.0.0:8080" \
  -e METRICS_PORT="9090" \
  -e RUST_LOG=info \
  --restart unless-stopped \
  --memory="256m" \
  --cpus="1.0" \
  -v dns_data:/app/data \
  rind-dns:latest
```

## Testing

```bash
# Health check
curl -s http://127.0.0.1:8080/
dig @127.0.0.1 -p 12312 localhost +short

# Add and query a record
curl -X POST http://127.0.0.1:8080/update \
  -H "Content-Type: application/json" \
  -d '{"name": "test.example.com", "ip": "192.168.1.100", "ttl": 300, "record_type": "A", "class": "IN", "value": null}'

dig @127.0.0.1 -p 12312 test.example.com

# Metrics
curl -s http://127.0.0.1:9090/metrics | grep dns_queries_total
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DNS_BIND_ADDR` | `0.0.0.0:12312` | DNS server bind address |
| `API_BIND_ADDR` | `0.0.0.0:8080` | REST API bind address |
| `METRICS_PORT` | `9090` | Prometheus metrics port |
| `SERVER_ID` | `dns-server-{PID}` | Instance identifier |
| `RUST_LOG` | `info` | Log level (debug, info, warn, error) |
| `LOG_FORMAT` | `json` | Log format (json, text) |

## Docker Compose

```bash
# Production stack
docker-compose -f docker/docker-compose.yml up

# Development
docker-compose -f docker/docker-compose.dev.yml up

# Full stack with monitoring
./scripts/start-fullstack.sh start
```

See [FULLSTACK.md](FULLSTACK.md) for the complete monitoring stack setup.

## Troubleshooting

**Container won't start:**
```bash
sudo lsof -i :12312    # check port conflicts
docker logs rind-server # check logs
```

**DNS queries timeout:**
- Verify server binds to `0.0.0.0:12312`, not `127.0.0.1:12312`
- Check `docker logs` for "DNS server listening" message
- Test UDP connectivity: `nc -u -v 127.0.0.1 12312 < /dev/null`

**Debug mode:**
```bash
docker run --rm -it \
  -p 12312:12312/udp -p 8080:8080/tcp \
  -e RUST_LOG=debug -e RUST_BACKTRACE=1 \
  rind-dns:latest
```

## Cleanup

```bash
docker stop rind-server && docker rm rind-server
docker rmi rind-dns:latest
docker system prune -f
```
