# RIND Project Restructuring Plan

## Current Issues
- Log files scattered in root and scripts/
- Runtime files mixed with source files
- Monitoring configs spread across directories
- Multiple Docker files without clear purpose

## Proposed Structure
```
RIND/
├── src/                    # Core source code (✓ already good)
├── tests/                  # Test suites (✓ already good)
├── benches/                # Performance benchmarks (✓ already good)
├── scripts/                # Build & deployment scripts (needs cleanup)
├── monitoring/             # All monitoring configs consolidated
│   ├── prometheus/
│   ├── grafana/
│   └── loki/
├── docker/                 # All Docker-related files
├── docs/                   # Documentation files
├── logs/                   # Runtime logs (gitignored)
└── tmp/                    # Temporary/runtime files (gitignored)
```

## Actions to Take
1. Create monitoring/ directory and consolidate configs
2. Create docker/ directory for Docker files
3. Create docs/ directory for documentation
4. Clean up scripts/ directory
5. Move log files to logs/ directory
6. Update .gitignore appropriately
7. Clean up root directory