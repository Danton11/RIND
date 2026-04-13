# RIND

RIND is a DNS server written in Rust. It speaks the DNS wire protocol over UDP and exposes a REST API for live record management, with Prometheus metrics built in. Designed to run as a primary/secondary pair behind a load balancer for HA deployments.

## Architecture

### Internals

A single RIND process runs three listeners against a shared record store:

```mermaid
graph LR
    UDP[UDP :12312<br/>DNS wire protocol] --> Store[(Record Store)]
    API[HTTP :8080<br/>REST API] --> Store
    Store --> Metrics[HTTP :9090<br/>Prometheus metrics]
    Store <--> Disk[(LMDB)]
```

- **UDP listener** — parses DNS queries, looks up the record store, encodes responses.
- **REST API** — CRUD over records, validated and applied to the same store.
- **Metrics endpoint** — exports query counts, latency histograms, error rates.
- **Persistence** — LMDB via `heed`. Every CRUD mutation is a single transaction covering the record store, the name index, a versioned changelog, and rolling state-hash metadata — so either all of it lands or none of it does. Chosen over a JSONL file for atomic multi-key writes, a durable changelog for replication, and no full-file rewrite on update.

### Deployment

Multiple instances compose into an HA setup behind HAProxy, with Prometheus, Grafana, Loki, and AlertManager providing observability:

```mermaid
graph TD
    Client[Clients]
    HAProxy[HAProxy<br/>DNS: port 53 UDP<br/>API: port 80 HTTP]
    Primary[RIND Primary<br/>DNS: 12312, API: 8080]
    Secondary[RIND Secondary<br/>DNS: 12313, API: 8081]
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

### Dashboards

| Dashboard | Screenshot |
|-----------|------------|
| DNS Server Overview | ![DNS Overview](docs/screenshots/dns-overview.png) |
| DNS Protocol Analysis | ![DNS Protocol](docs/screenshots/dns-protocol.png) |
| Record Management | ![Record Management](docs/screenshots/rind-record-management.png) |
| System Metrics | ![System Metrics](docs/screenshots/rind-system-metrics.png) |
| Error Analysis | ![Errors](docs/screenshots/dns-errors.png) |

See [docs/METRICS.md](docs/METRICS.md) for available metrics and PromQL queries.

## Documentation

- [Full Stack Deployment](docs/FULLSTACK.md) — Docker Compose setup with monitoring
- [Docker Guide](docs/DOCKER.md) — Building and running containers
- [Metrics](docs/METRICS.md) — Prometheus metrics, Grafana dashboards
- [System Metrics](docs/SYSTEM_METRICS_GUIDE.md) — Infrastructure-level monitoring
- [Remote Deployment](docs/REMOTE_DEPLOYMENT.md) — Deploying to a remote host

## License

MIT — see [LICENSE](LICENSE).
