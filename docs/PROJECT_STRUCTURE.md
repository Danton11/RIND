# RIND Project Structure

This document describes the reorganized structure of the RIND DNS server project.

## Directory Layout

```
RIND/
├── src/                    # Core source code
│   ├── main.rs            # Application entry point & server orchestration
│   ├── lib.rs             # Library exports
│   ├── server.rs          # DNS UDP server implementation
│   ├── packet.rs          # DNS packet parsing & response building
│   ├── query.rs           # DNS query handling logic
│   ├── update.rs          # Record management & file I/O
│   ├── metrics.rs         # Prometheus metrics implementation
│   └── bin/               # Utility binaries
│       ├── add_records.rs # DNS record addition utility
│       └── test_runner.rs # Comprehensive test runner
├── tests/                 # Test suites
│   ├── unit_packet_tests.rs    # DNS packet parsing unit tests
│   ├── unit_update_tests.rs    # Record management unit tests
│   └── integration_tests.rs    # End-to-end integration tests
├── benches/               # Performance benchmarks
│   └── dns_benchmarks.rs  # Criterion-based performance tests
├── scripts/               # Build & deployment scripts
│   ├── docker-build.sh    # Docker image build script
│   ├── dns-canary.py      # DNS monitoring canary
│   ├── start-canary.sh    # Canary startup script
│   └── test-monitoring.sh # Monitoring stack test script
├── docker/                # Docker configuration
│   ├── Dockerfile         # Production container image
│   ├── Dockerfile.dev     # Development container image
│   ├── Dockerfile.simple  # Simplified container image
│   ├── docker-compose.yml # Production compose configuration
│   ├── docker-compose.dev.yml # Development compose configuration
│   └── docker-compose.monitoring.yml # Monitoring stack configuration
├── monitoring/            # Monitoring and observability
│   ├── prometheus/        # Prometheus configuration
│   │   ├── prometheus.yml
│   │   └── record-management-alerts.yml
│   ├── grafana/          # Grafana dashboards and provisioning
│   │   ├── dashboards/
│   │   └── provisioning/
│   └── loki/             # Loki log aggregation configuration
├── docs/                  # Documentation
│   ├── README.md          # Main project documentation
│   ├── DOCKER.md          # Docker deployment guide
│   ├── METRICS.md         # Metrics and monitoring guide
│   ├── MONITORING.md      # Monitoring setup guide
│   ├── REMOTE_DEPLOYMENT.md # Remote deployment guide
│   └── PROJECT_STRUCTURE.md # This file
├── logs/                  # Runtime logs (gitignored)
├── tmp/                   # Temporary/runtime files (gitignored)
├── dns_records.txt        # DNS records storage file
├── Cargo.toml            # Rust package manifest
├── Cargo.lock            # Dependency lock file
└── .gitignore            # Git ignore rules
```

## Key Changes Made

### 1. Consolidated Monitoring
- All monitoring configurations moved to `monitoring/` directory
- Prometheus, Grafana, and Loki configs organized by service
- Updated Docker Compose files to reference new paths

### 2. Docker Organization
- All Docker-related files moved to `docker/` directory
- Updated build scripts to reference new Dockerfile locations
- Compose files updated with correct context paths

### 3. Documentation Centralization
- All markdown documentation moved to `docs/` directory
- Maintains project documentation in one place

### 4. Runtime File Management
- Log files moved to `logs/` directory (gitignored)
- Temporary files moved to `tmp/` directory (gitignored)
- Updated .gitignore to reflect new structure

### 5. Scripts Cleanup
- Removed runtime files (*.log, *.pid) from scripts directory
- Updated script paths to reference new structure
- Scripts now only contain actual executable files

## Usage After Restructuring

### Building with Docker
```bash
# From project root
./scripts/docker-build.sh

# Or manually
docker build -f docker/Dockerfile -t rind-dns:latest .
```

### Running with Docker Compose
```bash
# Production
docker-compose -f docker/docker-compose.yml up

# Development
docker-compose -f docker/docker-compose.dev.yml up

# Monitoring stack
docker-compose -f docker/docker-compose.monitoring.yml up
```

### Testing Monitoring
```bash
./scripts/test-monitoring.sh start
./scripts/test-monitoring.sh test
./scripts/test-monitoring.sh stop
```

## Benefits of New Structure

1. **Clear Separation of Concerns**: Each directory has a specific purpose
2. **Easier Navigation**: Related files are grouped together
3. **Cleaner Root Directory**: Less clutter in the main project directory
4. **Better Gitignore Management**: Runtime files properly excluded
5. **Docker Organization**: All Docker files in one place with proper context
6. **Centralized Documentation**: All docs in one location
7. **Monitoring Organization**: All observability configs consolidated

## Migration Notes

- All existing functionality remains the same
- Docker Compose commands now require `-f docker/` prefix
- Build scripts updated to use new paths
- Monitoring configurations moved but functionality unchanged
- Log files now properly organized and gitignored