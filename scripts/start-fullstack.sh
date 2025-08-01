#!/bin/bash

# RIND DNS Server - Full Stack Startup Script
# This script provides easy management of the complete RIND infrastructure

set -euo pipefail

# Configuration
COMPOSE_FILE="docker/docker-compose.fullstack.yml"
PROJECT_NAME="rind-fullstack"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging function
log() {
    echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[$(date +'%Y-%m-%d %H:%M:%S')] WARNING:${NC} $1"
}

error() {
    echo -e "${RED}[$(date +'%Y-%m-%d %H:%M:%S')] ERROR:${NC} $1"
}

# Help function
show_help() {
    cat << EOF
RIND DNS Server - Full Stack Management

Usage: $0 [COMMAND] [OPTIONS]

COMMANDS:
    start           Start the full stack (default)
    stop            Stop all services
    force-stop      Force stop all services (emergency cleanup)
    restart         Restart all services
    status          Show service status
    logs            Show logs for all services
    logs <service>  Show logs for specific service
    build           Build all images
    clean           Clean up containers and volumes
    test            Run test suite
    benchmark       Run performance benchmarks
    health          Check health of all services
    scale           Scale DNS servers
    backup          Backup configuration and data
    restore         Restore from backup

PROFILES:
    --production    Include production services (HAProxy, AlertManager)
    --testing       Include testing services
    --utilities     Include utility services
    --benchmarking  Include benchmark services
    --dev           Development mode with hot reload

EXAMPLES:
    $0 start --production          # Start with production services
    $0 logs dns-server-primary     # Show logs for primary DNS server
    $0 scale 3                     # Scale to 3 DNS server instances
    $0 test                        # Run comprehensive test suite
    $0 benchmark                   # Run performance benchmarks

MONITORING URLS:
    Grafana:        http://localhost:3000 (admin/rind-admin-204)
    Prometheus:     http://localhost:9090
    AlertManager:   http://localhost:9093
    HAProxy Stats:  http://localhost:8404/stats
    DNS API:        http://localhost:80 (load balanced)
    DNS Server:     udp://localhost:53 (load balanced)

EOF
}

# Check prerequisites
check_prerequisites() {
    log "Checking prerequisites..."
    
    if ! command -v docker &> /dev/null; then
        error "Docker is not installed or not in PATH"
        exit 1
    fi
    
    if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
        error "Docker Compose is not installed"
        exit 1
    fi
    
    # Check if we're in the right directory
    if [[ ! -f "$ROOT_DIR/Cargo.toml" ]]; then
        error "Please run this script from the RIND project root directory"
        exit 1
    fi
    
    log "Prerequisites check passed ✓"
}

# Build images
build_images() {
    log "Building RIND Docker images..."
    cd "$ROOT_DIR"
    
    docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" build --parallel
    
    log "Images built successfully ✓"
}

# Start services
start_services() {
    local profiles=""
    
    # Parse profile arguments
    for arg in "$@"; do
        case $arg in
            --production)
                profiles="$profiles --profile production"
                ;;
            --testing)
                profiles="$profiles --profile testing"
                ;;
            --utilities)
                profiles="$profiles --profile utilities"
                ;;
            --benchmarking)
                profiles="$profiles --profile benchmarking"
                ;;
            --dev)
                warn "Development mode not available in fullstack. Use docker-compose.dev.yml instead"
                ;;
        esac
    done
    
    log "Starting RIND full stack..."
    cd "$ROOT_DIR"
    
    # Create necessary directories
    mkdir -p logs monitoring/{prometheus,grafana,loki,alertmanager,haproxy}
    
    # Start services
    if [[ -n "$profiles" ]]; then
        docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" $profiles up -d
    else
        docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" up -d
    fi
    
    log "Services started successfully ✓"
    
    # Wait for services to be healthy
    log "Waiting for services to become healthy..."
    sleep 10
    
    # Show service status
    show_status
    
    # Show access URLs
    show_urls
}

# Force stop services (emergency cleanup)
force_stop_services() {
    log "Force stopping all RIND services..."
    
    # Stop all RIND containers
    local containers=$(docker ps -aq --filter "name=rind-")
    if [[ -n "$containers" ]]; then
        log "Stopping containers..."
        echo "$containers" | xargs -r docker stop
        echo "$containers" | xargs -r docker rm
    fi
    
    # Remove networks
    local networks=$(docker network ls -q --filter "name=rind-")
    if [[ -n "$networks" ]]; then
        log "Removing networks..."
        echo "$networks" | xargs -r docker network rm
    fi
    
    log "Force stop completed ✓"
}

# Stop services
stop_services() {
    log "Stopping RIND full stack..."
    cd "$ROOT_DIR"
    
    # Try graceful shutdown first
    docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" down
    
    # Check if any containers are still running
    local remaining=$(docker ps -q --filter "name=rind-")
    if [[ -n "$remaining" ]]; then
        warn "Some containers still running, force stopping..."
        echo "$remaining" | xargs -r docker stop
        echo "$remaining" | xargs -r docker rm
    fi
    
    # Clean up network if it still exists
    local network=$(docker network ls -q --filter "name=rind-fullstack")
    if [[ -n "$network" ]]; then
        warn "Cleaning up remaining network..."
        docker network rm rind-fullstack 2>/dev/null || true
    fi
    
    log "Services stopped successfully ✓"
}

# Restart services
restart_services() {
    log "Restarting RIND full stack..."
    stop_services
    sleep 5
    start_services "$@"
}

# Show service status
show_status() {
    log "Service Status:"
    cd "$ROOT_DIR"
    
    docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" ps
    
    echo
    log "Health Checks:"
    
    # Check DNS servers
    if curl -s -f "http://localhost:8080/records" > /dev/null 2>&1; then
        echo -e "  ${GREEN}✓${NC} dns-server-primary API is healthy"
    else
        echo -e "  ${RED}✗${NC} dns-server-primary API is not responding"
    fi
    
    if curl -s -f "http://localhost:8081/records" > /dev/null 2>&1; then
        echo -e "  ${GREEN}✓${NC} dns-server-secondary API is healthy"
    else
        echo -e "  ${RED}✗${NC} dns-server-secondary API is not responding"
    fi
    
    # Check monitoring services
    services=(
        "prometheus:9090:-/healthy"
        "grafana:3000:api/health"
        "loki:3100:ready"
    )
    
    for service_info in "${services[@]}"; do
        IFS=':' read -r service port path <<< "$service_info"
        if curl -s -f "http://localhost:$port/$path" > /dev/null 2>&1; then
            echo -e "  ${GREEN}✓${NC} $service is healthy"
        else
            echo -e "  ${RED}✗${NC} $service is not responding"
        fi
    done
}

# Show logs
show_logs() {
    cd "$ROOT_DIR"
    
    if [[ $# -eq 0 ]]; then
        # Show all logs
        docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" logs -f --tail=100
    else
        # Show specific service logs
        docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" logs -f --tail=100 "$1"
    fi
}

# Clean up
clean_up() {
    log "Cleaning up RIND full stack..."
    cd "$ROOT_DIR"
    
    # Stop and remove containers with volumes
    docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" down -v --remove-orphans
    
    # Force stop any remaining RIND containers
    local remaining=$(docker ps -aq --filter "name=rind-")
    if [[ -n "$remaining" ]]; then
        warn "Force removing remaining containers..."
        echo "$remaining" | xargs -r docker stop
        echo "$remaining" | xargs -r docker rm
    fi
    
    # Remove RIND images
    local images=$(docker images --filter "reference=rind-*" -q)
    if [[ -n "$images" ]]; then
        warn "Removing RIND images..."
        echo "$images" | xargs -r docker rmi -f
    fi
    
    # Clean up networks
    local networks=$(docker network ls -q --filter "name=rind-")
    if [[ -n "$networks" ]]; then
        warn "Removing RIND networks..."
        echo "$networks" | xargs -r docker network rm
    fi
    
    # Clean up volumes
    local volumes=$(docker volume ls -q --filter "name=rind-fullstack")
    if [[ -n "$volumes" ]]; then
        warn "Removing RIND volumes..."
        echo "$volumes" | xargs -r docker volume rm
    fi
    
    # Clean up Docker system
    docker system prune -f
    
    log "Cleanup completed ✓"
}

# Run tests
run_tests() {
    log "Running RIND test suite..."
    cd "$ROOT_DIR"
    
    # Start test profile
    docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" --profile testing up -d
    
    # Wait for services
    sleep 15
    
    # Run tests
    docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" run --rm dns-tester
    
    log "Tests completed ✓"
}

# Run benchmarks
run_benchmarks() {
    log "Running RIND performance benchmarks..."
    cd "$ROOT_DIR"
    
    # Start benchmark profile
    docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" --profile benchmarking up -d
    
    # Wait for services
    sleep 15
    
    # Run benchmarks
    docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" run --rm benchmark-runner
    
    log "Benchmarks completed ✓"
}

# Scale DNS servers
scale_servers() {
    local count=${1:-2}
    log "Scaling DNS servers to $count instances..."
    cd "$ROOT_DIR"
    
    docker-compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" up -d --scale dns-server-primary=1 --scale dns-server-secondary=$((count-1))
    
    log "Scaled to $count DNS server instances ✓"
}

# Show access URLs
show_urls() {
    echo
    log "Access URLs:"
    echo -e "  ${BLUE}Grafana Dashboard:${NC}    http://localhost:3000 (admin/rind-admin-2025)"
    echo -e "  ${BLUE}Prometheus Metrics:${NC}   http://localhost:9090"
    echo -e "  ${BLUE}AlertManager:${NC}         http://localhost:9093"
    echo -e "  ${BLUE}HAProxy Stats:${NC}        http://localhost:8404/stats"
    echo -e "  ${BLUE}DNS API (Load Balanced):${NC} http://localhost:80"
    echo -e "  ${BLUE}DNS Server:${NC}           udp://localhost:53"
    echo -e "  ${BLUE}Primary DNS API:${NC}      http://localhost:8080"
    echo -e "  ${BLUE}Secondary DNS API:${NC}    http://localhost:8081"
    echo
}

# Main execution
main() {
    check_prerequisites
    
    case "${1:-start}" in
        start)
            shift
            start_services "$@"
            ;;
        stop)
            stop_services
            ;;
        restart)
            shift
            restart_services "$@"
            ;;
        status)
            show_status
            ;;
        logs)
            shift
            show_logs "$@"
            ;;
        build)
            build_images
            ;;
        clean)
            clean_up
            ;;
        force-stop)
            force_stop_services
            ;;
        test)
            run_tests
            ;;
        benchmark)
            run_benchmarks
            ;;
        health)
            show_status
            ;;
        scale)
            scale_servers "${2:-2}"
            ;;
        urls)
            show_urls
            ;;
        help|--help|-h)
            show_help
            ;;
        *)
            error "Unknown command: $1"
            echo
            show_help
            exit 1
            ;;
    esac
}

# Run main function with all arguments
main "$@"