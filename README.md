# RIND

A DNS server written in Rust with a REST API for record management, designed to run as a primary/secondary pair behind HAProxy with full observability via Prometheus, Grafana, and Loki.

## Architecture

```mermaid
graph TD
    Client[Clients]
    HAProxy[HAProxy<br/>DNS: port 53 UDP<br/>API: port 80 HTTP]
    Primary[DNS Server Primary<br/>DNS: 12312, API: 8080]
    Secondary[DNS Server Secondary<br/>DNS: 12313, API: 8081]
    Prometheus[Prometheus<br/>port 9090]
    Grafana[Grafana<br/>port 3000]
    Loki[Loki<br/>port 3100]
    AlertManager[AlertManager<br/>port 9093]

    Client --> HAProxy
    HAProxy --> Primary
    HAProxy --> Secondary
    Primary -->|metrics| Prometheus
    Secondary -->|metrics| Prometheus
    Primary -->|logs| Loki
    Secondary -->|logs| Loki
    Prometheus --> Grafana
    Loki --> Grafana
    Prometheus --> AlertManager
```

Each DNS server instance runs three listeners:
- **UDP** — DNS protocol (port 12312)
- **HTTP** — REST API for record CRUD (port 8080)
- **HTTP** — Prometheus metrics (port 9090)

Records are stored in a flat file (`dns_records.txt`), with plans to migrate to LMDB.

## Quick Start

```bash
# Full stack with monitoring
./scripts/start-fullstack.sh start

# Or native development
cargo run
```

### Test it

```bash
# Add a record
curl -X POST http://localhost:8080/records \
  -H "Content-Type: application/json" \
  -d '{"name": "example.com", "ip": "93.184.216.34", "ttl": 300}'

# Query it
dig @localhost -p 12312 example.com

# List records
curl http://localhost:8080/records
```

## API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/records` | Create a record |
| `GET` | `/records` | List records (paginated) |
| `PUT` | `/records/:name` | Update a record |
| `DELETE` | `/records/:name` | Delete a record |

## Development

```bash
cargo build --release
cargo test
cargo bench
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

CI runs `fmt --check`, `clippy`, and `test` on every push/PR via GitHub Actions.

## Monitoring

The full stack includes Prometheus, Grafana, Loki, and AlertManager. After starting:

- **Grafana**: http://localhost:3000 (credentials in `.env`, see `.env.example`)
- **Prometheus**: http://localhost:9090
- **HAProxy Stats**: http://localhost:8404/stats

See [docs/METRICS.md](docs/METRICS.md) for available metrics and PromQL queries.

## Documentation

- [Full Stack Deployment](docs/FULLSTACK.md) — Docker Compose setup with monitoring
- [Docker Guide](docs/DOCKER.md) — Building and running containers
- [Metrics](docs/METRICS.md) — Prometheus metrics, Grafana dashboards
- [System Metrics](docs/SYSTEM_METRICS_GUIDE.md) — Infrastructure-level monitoring
- [Remote Deployment](docs/REMOTE_DEPLOYMENT.md) — Deploying to a remote host

## License

MIT — see [LICENSE](LICENSE).
