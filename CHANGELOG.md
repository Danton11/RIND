# Changelog

## [Unreleased]

- LMDB storage scaffolding: new `src/storage.rs` module with `LmdbStore`
  handle backed by a heed environment. Four databases (`records`,
  `records_by_name`, `zones`, `changelog`, `metadata`) opened atomically.
  Transactional per-record CRUD with rolling FNV-1a-128 state hash for
  drift detection, monotonically increasing version counter, and an
  on-disk `schema_version` check that fails fast on mismatch.
- Zone model groundwork: `Zone` struct mirroring SOA fields, zone CRUD
  methods, and `find_zone_for()` longest-suffix matching so the query
  path can answer AA/REFUSED correctly once Phase 2A lands.
- RFC 4343 compliance in the storage index: names are canonicalized
  (ASCII lowercase, trailing dot stripped) on every write and lookup so
  `EXAMPLE.com.` and `example.com` hit the same slot. Stored record
  bodies keep their original case — case-preserving, case-insensitive.
- New operator doc `docs/KUBERNETES.md` covering LMDB filesystem
  requirements (block- vs file-level network storage, CSI driver check),
  cross-architecture snapshot warning, map-size tuning, and StatefulSet
  vs Deployment pod topology.
- Configurable LMDB map size via `RIND_LMDB_MAP_SIZE` env var (default
  1 GiB). Invalid values are a hard startup error — no silent fallback.
- Moved Grafana credentials to environment variables (`.env`)
- **Breaking**: DNS records now use a typed `RecordData` enum. A and AAAA records
  are both first-class; the wire-format query path filters on both name and
  qtype, returning NODATA (NOERROR, ANCOUNT=0) when a name exists but the
  requested type does not.
- **Breaking**: REST API payload shape flattened. `POST /records` now takes
  `{"name","ttl","class","type":"A"|"AAAA","ip"}`. `PUT /records/{id}` takes
  partial fields plus an optional nested `"data": {"type":..., "ip":...}` —
  omit `data` to preserve the existing payload, include it to replace wholesale.
  The legacy `/update` endpoint is removed.
- **Breaking**: Record storage format switched from the custom colon format to
  JSON Lines. Default file path is now `dns_records.jsonl`. The pluggable
  `DatastoreProvider` trait is wired through startup + CRUD so alternative
  backends can slot in without touching handlers. `FileDatastoreProvider` is
  renamed to `JsonlFileDatastoreProvider`.

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
