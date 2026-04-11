# Changelog

## [Unreleased]

- Moved Grafana credentials to environment variables (`.env`)

## [0.1.0] - 2026-04-11

Initial tagged release. Draws a line under the existing state before LMDB migration work begins.

### Added
- DNS server with UDP listener and A record resolution
- REST API for record CRUD (POST, GET, PUT, DELETE)
- File-based record persistence (`dns_records.txt`)
- Prometheus metrics endpoint per server instance
- Grafana dashboards (overview, canary, system, protocol, record management, errors)
- Loki log aggregation with structured JSON logging
- AlertManager integration
- HAProxy load balancing (DNS UDP + API HTTP) with health checks
- Primary/secondary DNS server deployment via Docker Compose
- System metrics exporter (Python) for process/container monitoring
- DNS canary monitoring script
- GitHub Actions CI (fmt, clippy, tests)
- MIT license
- Criterion benchmarks for packet parsing and response building
- Integration test suite (server-dependent, marked `#[ignore]`)
- Unit tests for packet parsing, record management, and API endpoints
- Docker dependency caching layer for faster rebuilds

### Changed
- Rewrote all documentation: removed AI-generated fluff, consolidated redundant files
- README now has real architecture description with Mermaid diagram

### Removed
- `dns_records.txt` from version control (runtime data, now gitignored)
- Redundant docs: DASHBOARDS.md, GRAFANA_METRICS_GUIDE.md, MONITORING.md, PROJECT_STRUCTURE.md
