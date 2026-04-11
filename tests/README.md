# Tests

All tests are in Rust. No external dependencies required.

## Running

```bash
# All unit tests
cargo test

# Integration tests (require running server)
cargo test --test integration_tests -- --ignored

# Individual suites
cargo test --test unit_packet_tests
cargo test --test unit_update_tests

# Benchmarks
cargo bench --bench dns_benchmarks
```

## Test Files

### Unit Tests
- `unit_packet_tests.rs` — DNS packet parsing and response building
- `unit_update_tests.rs` — Record file I/O and management

### Integration Tests
- `integration_tests.rs` — end-to-end API-to-DNS resolution, load testing, malformed packets. Marked `#[ignore]` because they need a running server.

### Endpoint Tests
- `test_post_endpoint.rs` — POST /records
- `test_put_endpoint.rs` — PUT /records
- `test_delete_endpoint.rs` — DELETE /records
- `test_list_records_endpoint.rs` — GET /records

### Monitoring Tests
- `monitoring_integration_tests.rs` — metrics exposure, Prometheus scraping, Grafana, Loki. See [MONITORING_TESTS.md](MONITORING_TESTS.md).

### Benchmarks
- `benches/dns_benchmarks.rs` — Criterion benchmarks for packet parsing (~100ns) and response building (~520ns)

## Server Requirements

Integration tests need the server running:
```bash
cargo run  # DNS on UDP 12312, API on HTTP 8080
```
