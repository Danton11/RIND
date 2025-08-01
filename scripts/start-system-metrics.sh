#!/bin/bash

# RIND System Metrics Exporter Startup Script
# Starts the system metrics exporter with proper configuration

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
METRICS_PORT="${METRICS_PORT:-8091}"
METRICS_HOST="${METRICS_HOST:-0.0.0.0}"
LOG_LEVEL="${LOG_LEVEL:-INFO}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_debug() {
    echo -e "${BLUE}[DEBUG]${NC} $1"
}

# Function to check if Python dependencies are installed
check_dependencies() {
    log_info "Checking Python dependencies..."
    
    if ! command -v python3 &> /dev/null; then
        log_error "Python 3 is not installed"
        exit 1
    fi
    
    # Check for required Python packages
    local required_packages=("psutil" "requests")
    local missing_packages=()
    
    for package in "${required_packages[@]}"; do
        if ! python3 -c "import $package" &> /dev/null; then
            missing_packages+=("$package")
        fi
    done
    
    if [ ${#missing_packages[@]} -ne 0 ]; then
        log_warn "Missing Python packages: ${missing_packages[*]}"
        log_info "Installing missing packages..."
        pip3 install "${missing_packages[@]}" || {
            log_error "Failed to install Python packages"
            exit 1
        }
    fi
    
    log_info "All dependencies are satisfied"
}

# Function to check if the metrics exporter script exists
check_script() {
    local script_path="$PROJECT_ROOT/scripts/system-metrics-exporter.py"
    
    if [ ! -f "$script_path" ]; then
        log_error "System metrics exporter script not found at: $script_path"
        exit 1
    fi
    
    if [ ! -x "$script_path" ]; then
        log_info "Making script executable..."
        chmod +x "$script_path"
    fi
    
    log_info "System metrics exporter script found and executable"
}

# Function to check if port is available
check_port() {
    if netstat -tuln 2>/dev/null | grep -q ":$METRICS_PORT "; then
        log_error "Port $METRICS_PORT is already in use"
        log_info "Use 'METRICS_PORT=<port> $0' to specify a different port"
        exit 1
    fi
    
    log_info "Port $METRICS_PORT is available"
}

# Function to start the metrics exporter
start_exporter() {
    local script_path="$PROJECT_ROOT/scripts/system-metrics-exporter.py"
    
    log_info "Starting RIND System Metrics Exporter..."
    log_info "Host: $METRICS_HOST"
    log_info "Port: $METRICS_PORT"
    log_info "Log Level: $LOG_LEVEL"
    
    # Set environment variables
    export METRICS_PORT="$METRICS_PORT"
    export METRICS_HOST="$METRICS_HOST"
    export PYTHONUNBUFFERED=1
    
    # Start the exporter
    cd "$PROJECT_ROOT"
    exec python3 "$script_path"
}

# Function to show usage
show_usage() {
    cat << EOF
RIND System Metrics Exporter Startup Script

Usage: $0 [OPTIONS]

Options:
    -h, --help          Show this help message
    -p, --port PORT     Set metrics port (default: 8091)
    -H, --host HOST     Set metrics host (default: 0.0.0.0)
    -l, --log-level LVL Set log level (default: INFO)
    --check-only        Only check dependencies and configuration

Environment Variables:
    METRICS_PORT        Metrics server port
    METRICS_HOST        Metrics server host
    LOG_LEVEL          Logging level

Examples:
    $0                                  # Start with default settings
    $0 -p 8092                         # Start on port 8092
    $0 -H 127.0.0.1 -p 8091           # Start on localhost:8091
    METRICS_PORT=8093 $0               # Start using environment variable

The metrics exporter will be available at:
    http://\$METRICS_HOST:\$METRICS_PORT/metrics
    http://\$METRICS_HOST:\$METRICS_PORT/health

EOF
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_usage
            exit 0
            ;;
        -p|--port)
            METRICS_PORT="$2"
            shift 2
            ;;
        -H|--host)
            METRICS_HOST="$2"
            shift 2
            ;;
        -l|--log-level)
            LOG_LEVEL="$2"
            shift 2
            ;;
        --check-only)
            CHECK_ONLY=true
            shift
            ;;
        *)
            log_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# Main execution
main() {
    log_info "RIND System Metrics Exporter Startup"
    log_info "======================================"
    
    # Perform checks
    check_dependencies
    check_script
    check_port
    
    if [ "${CHECK_ONLY:-false}" = "true" ]; then
        log_info "Configuration check completed successfully"
        exit 0
    fi
    
    # Start the exporter
    start_exporter
}

# Handle signals for graceful shutdown
trap 'log_info "Received shutdown signal, exiting..."; exit 0' SIGTERM SIGINT

# Run main function
main "$@"