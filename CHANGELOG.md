# Changelog

## [Unreleased]

- New `LmdbStore::put_records_batch(&[DnsRecord])` writes N records in one
  `RwTxn` and commits once. Each record still produces its own version bump
  and changelog entry, so the log stays one-entry-per-mutation — but the
  single `fdatasync` at commit amortizes across the whole batch. On btrfs,
  per-record cost drops from ~3.8 ms (single commit) to ~20 µs at batch
  size 1000 (~190× speedup). Intended for bulk-write paths; no REST caller
  yet. `put_record` now delegates to a shared `put_record_in_txn` helper.
- DNS query path reads directly from LMDB via the `records_by_name`
  secondary index. The `Arc<RwLock<DnsRecords>>` in-memory cache is gone
  along with the `DatastoreProvider` trait and its in-memory test stub;
  every read is a fresh LMDB prefix scan, and every write lands through
  a single `Arc<LmdbStore>` handle. Handlers, the server dispatch loop,
  and `build_instance` all thread the store directly. No more
  cache-coherence bug surface, no more "stub behaves like LMDB"
  assumptions in tests.
- REST handler integration tests (`tests/test_post_endpoint.rs`,
  `tests/test_put_endpoint.rs`, `tests/test_delete_endpoint.rs`,
  `tests/test_list_records_endpoint.rs`) now run against a real
  in-process RIND instance via `TestHarness::spawn()` — real sockets,
  real warp filters, real LMDB tempdir — instead of re-implementing a
  second copy of the routing on top of an in-memory stub.
- `DELETE /records/:id` now returns an empty `204 No Content` body per
  RFC 7230 §3.3.3. Previously we emitted a JSON success envelope with a
  204 status, which some strict clients reject.
- Changelog entries are now written to LMDB in the same `RwTxn` as the
  record mutation itself. Every `put_record` / `delete_record_by_id`
  commit produces exactly one `ChangelogEntry` keyed by the bumped
  version counter, tagged Create / Update / Delete. This is the write
  half of the sync protocol that secondaries will consume later.
- **Breaking**: LMDB is now the persistence backend. `JsonlFileDatastoreProvider`
  and the `dns_records.jsonl` on-disk format are gone. The DNS server opens an
  LMDB environment at `$RIND_LMDB_PATH` (or `$DATA_DIR/lmdb` as a fallback,
  created on first boot) and persists every CRUD mutation as its own LMDB
  transaction. Existing `dns_records.jsonl` files are ignored — recreate
  records through the REST API.
- `DatastoreProvider` trait reshaped around per-record operations:
  `put_record(&DnsRecord)` and `delete_record(&str)` replace the old
  bulk `save_all_records`. CRUD handlers persist first and update the
  in-memory cache only on success, so a failed write no longer leaves a
  phantom cache entry. Errors flow through a new `DatastoreError` enum
  instead of boxed `dyn Error`.
- LMDB storage scaffolding: new `src/storage.rs` module with `LmdbStore`
  handle backed by a heed environment. Five databases (`records`,
  `records_by_name`, `zones`, `changelog`, `metadata`) opened atomically.
  Transactional per-record CRUD with rolling FNV-1a-128 state hash for
  drift detection, monotonically increasing version counter, and an
  on-disk `schema_version` check that fails fast on mismatch.
- Zone model groundwork: `Zone` struct mirroring SOA fields, zone CRUD
  methods, and `find_zone_for()` longest-suffix matching so the query
  path can answer AA/REFUSED correctly once zones are wired into the
  query layer.
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
- CNAME records: new `RecordData::Cname { target }` variant served over both
  REST and UDP. Write path enforces RFC 2181 §10.1 — a name holding a CNAME
  cannot hold any other record type, and vice versa (409 on conflict). Wire
  encoder emits uncompressed target names per RFC 1035 §3.3.1.
- PTR records: new `RecordData::Ptr { target }` variant (type code 12, RFC
  1035 §3.3.12). Same wire shape as CNAME; no exclusivity rule, standard
  singleton `(name, type)` semantics. Zone membership (in-addr.arpa /
  ip6.arpa) is not enforced.
- NS records and multi-value RRSet support: new `RecordData::Ns { target }`
  variant (type code 2, RFC 1035 §3.3.11). Multiple NS records at the same
  name are now permitted — this is the delegation set model required for
  real DNS. The query path returns all matching records for a qtype, not
  just the first, so `dig example.com NS` emits ANCOUNT=N. RFC 2181 §5.2
  uniform-TTL rule enforced at read time via min-clamping across the RRSet.
  RFC 2181 §5 RRSet-as-set invariant enforced at write time: exact-rdata
  duplicates within an RRSet are rejected with 409.
- **Breaking (internal API)**: `packet::build_response` signature changed
  from `(query, Option<&RecordData>, u8, u32)` to `(query, &[(&RecordData,
  u32)], u8)`. Per-answer TTL lives in the slice; empty slice means
  NODATA/NXDOMAIN/FORMERR. Callers outside the crate (there are none in
  tree) will need to update.
- New write-path policy hook: `RecordData::allows_multiple()`. Returns
  `true` for NS/MX/TXT, `false` for A/AAAA/CNAME/PTR. Flip the match arm
  on A/AAAA to enable round-robin DNS if you want it.
- MX records: new `RecordData::Mx { preference, exchange }` variant (type
  code 15, RFC 1035 §3.3.9). First numeric rdata field — `preference` is
  a `u16`. Multi-value: the usual primary + fallback pattern. Clients sort
  by preference per RFC 974; server does not. Null MX (RFC 7505, exchange
  = ".") is not currently supported — the name encoder doesn't handle
  root-label names correctly.
- TXT records: new `RecordData::Txt { strings }` variant (type code 16,
  RFC 1035 §3.3.14). First collection rdata — `strings` is a `Vec<String>`,
  each element a character-string emitted with a 1-byte length prefix on
  the wire. Multi-value at the RRSet level. Two write-time validations:
  empty `strings` Vec is rejected (degenerate record), and per-string
  bytes are capped at 255 (RFC length-field limit). Users with longer
  values split manually into multiple Vec entries.
- New rdata-level validation hook: `RecordData::validate_rdata()` called
  from `DnsRecord::validate`. Currently only TXT has rules; the seam is
  there for SOA/SRV/CAA and the like.
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
