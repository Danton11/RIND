# DNS Server Test Suite

This directory contains comprehensive tests for the DNS server implementation, now **fully migrated to Rust** for better performance and type safety.

## Test Files

### Unit Tests (Rust)

- **`unit_packet_tests.rs`** - DNS packet parsing and response building tests
- **`unit_update_tests.rs`** - DNS record file I/O and management tests

### Integration Tests (Rust)

- **`integration_tests.rs`** - Complete end-to-end testing including:
  - End-to-end API to DNS resolution timing
  - High-load stress testing with concurrent queries
  - DNS protocol edge cases and malformed packets
  - Extreme load testing and resource exhaustion
  - Record update performance testing
  - Sustained load testing

### Performance Benchmarks

- **`../benches/dns_benchmarks.rs`** - Criterion-based micro-benchmarks:
  - DNS packet parsing performance (~100ns)
  - Response building performance (~520ns)
  - Record file operations (load/save)
  - Concurrent processing benchmarks

### Utility Binaries

- **`../src/bin/add_records.rs`** - Add meaningful DNS records (Rust version)
- **`../src/bin/test_runner.rs`** - Comprehensive test suite runner



## Prerequisites

**No external dependencies required!** Everything runs with Rust's built-in toolchain.

## Running Tests

### All Tests (Recommended)

```bash
# Comprehensive test suite with automatic server detection
cargo run --bin test_runner
```

### Individual Test Categories

```bash
# Unit tests
cargo test --test unit_packet_tests
cargo test --test unit_update_tests

# Integration tests (all-in-one)
cargo test --test integration_tests

# Performance benchmarks
cargo bench --bench dns_benchmarks

# Add test records
cargo run --bin add_records
```



## Test Categories

### 🎯 End-to-End Tests
- API record addition timing
- DNS resolution speed
- Record update propagation
- Concurrent operations

### 🔥 Stress Tests
- 500+ concurrent DNS queries
- 50+ concurrent API updates
- 30-second sustained load
- Memory leak detection

### 🧪 Edge Case Tests
- Malformed DNS packets
- Invalid domain names
- Various packet sizes
- Protocol compliance

### 💥 Extreme Tests
- 21,000+ QPS flood testing
- Connection exhaustion
- Resource limit testing
- Attack simulation

## Expected Performance

- **End-to-End Time**: ~12ms (API → DNS resolution)
- **DNS Query Time**: <2ms average
- **API Response Time**: ~10ms average
- **Concurrent Success**: 100% under normal load
- **Max Throughput**: 21,000+ QPS sustained

## Server Requirements

Make sure the DNS server is running before executing tests:

```bash
# Start the server
cargo run

# Server should be listening on:
# - DNS: UDP 127.0.0.1:12312
# - API: HTTP 127.0.0.1:8080
```

## Test Results Interpretation

### ✅ Success Indicators
- All queries return expected responses
- No timeouts or connection errors
- Consistent response times
- 100% success rate under normal load

### ⚠️ Warning Signs
- Response times >100ms
- Success rate <95%
- Memory usage growth
- Connection failures

### ❌ Failure Conditions
- Server crashes or hangs
- Malformed responses
- Data corruption
- Resource exhaustion