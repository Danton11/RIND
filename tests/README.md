# Tests

All tests are in Rust. No external services required.

## Running

```bash
cargo test                                    # everything
cargo test --lib                              # lib unit tests only
cargo test --test integration_tests           # in-process end-to-end
cargo test --test unit_packet_tests           # one suite
cargo bench --bench dns_benchmarks            # criterion benches
```

## Layout

### Unit tests
- `unit_packet_tests.rs` — DNS packet parsing and response building
- `unit_query_tests.rs` — query path (A / AAAA / NS / NODATA / NXDOMAIN)

### Endpoint tests (filter-level, no server)
- `test_post_endpoint.rs` — POST /records
- `test_put_endpoint.rs` — PUT /records
- `test_delete_endpoint.rs` — DELETE /records
- `test_list_records_endpoint.rs` — GET /records
- `test_uuid_functionality.rs` — record ID generation

### Integration tests
- `integration_tests.rs` — end-to-end through a real in-process RIND
  instance via `common::harness::TestHarness` (ephemeral DNS + REST ports,
  fresh LMDB tempdir). Runs in ~1s, no `#[ignore]`, no docker. Includes
  `/health` readiness-gating tests that flip the `ready` flag the harness
  exposes.

### Kubernetes-feature tests (built only with `--features kubernetes`)
- `src/crd.rs` — `CrdRecordData` ↔ `RecordData` round-trips for all 9
  record types, plus drift tests asserting `EXPECTED_RECORD_TYPES` matches
  both the Rust enum and the YAML CRD schemas (kustomize and Helm).
- `src/update.rs::validate_against_store_tests` — the shared validator
  used by both the standalone CRUD path and the kubernetes REST shim
  (singleton dup, RRSet rules, CNAME exclusivity, `exclude_id` semantics).

### Cluster integration (out-of-process)
- `scripts/test-migrate-to-crd.sh` — fixture test for the migration
  script. Pipes one record per supported type and asserts type-specific
  fields render correctly.
- `scripts/ci-local.sh smoke` — full k3d cluster smoke: brings up the
  cluster, applies sample records, asserts dig answers for all 9 record
  types, exercises REST POST → CRD → DNS roundtrip + CNAME-over-A 409.
  Mirrors the `k3d-smoke` job in `.github/workflows/ci.yml`.

### Shared helpers
- `common/harness.rs` — `TestHarness::spawn()` + query/create helpers,
  plus the `ready: Arc<AtomicBool>` flag for `/health` tests.

### Benchmarks
- `benches/dns_benchmarks.rs` — packet parsing (~100ns) and response building (~520ns)

## Fullstack verification

The observability pipeline (Prometheus scrape, Grafana dashboards, Loki
aggregation, HAProxy routing) is not covered by `cargo test` — those live
outside the process. Exercise them via `./scripts/start-fullstack.sh` and
inspect the dashboards directly. See CLAUDE.md "Testing Against the Real
Server" for the three-tier testing model.
