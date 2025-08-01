#!/bin/bash

# Test script for monitoring stack integration tests
# This script helps set up and run the monitoring integration tests

set -e

echo "=== DNS Server Monitoring Integration Tests ==="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    print_error "Docker is not running. Please start Docker first."
    exit 1
fi

# Check if docker-compose is available
if ! command -v docker-compose &> /dev/null; then
    print_error "docker-compose is not installed or not in PATH"
    exit 1
fi

# Function to check if monitoring stack is running
check_monitoring_stack() {
    print_status "Checking if monitoring stack is running..."
    
    # Check if containers are running
    local containers=("prometheus" "grafana" "loki" "promtail" "dns-server-1" "dns-server-2")
    local running_containers=0
    
    for container in "${containers[@]}"; do
        if docker ps --format "table {{.Names}}" | grep -q "^${container}$"; then
            print_status "✓ $container is running"
            ((running_containers++))
        else
            print_warning "✗ $container is not running"
        fi
    done
    
    if [ $running_containers -eq ${#containers[@]} ]; then
        print_status "All monitoring stack containers are running"
        return 0
    else
        print_warning "$running_containers/${#containers[@]} containers are running"
        return 1
    fi
}

# Function to start monitoring stack
start_monitoring_stack() {
    print_status "Starting monitoring stack..."
    
    # Build DNS server image first
    print_status "Building DNS server image..."
    docker build -t rind-dns:latest .
    
    # Start the monitoring stack
    print_status "Starting monitoring services..."
    docker-compose -f docker/docker-compose.monitoring.yml up -d
    
    # Wait for services to be ready
    print_status "Waiting for services to start..."
    sleep 30
    
    # Check service health
    local max_attempts=12
    local attempt=1
    
    while [ $attempt -le $max_attempts ]; do
        print_status "Health check attempt $attempt/$max_attempts..."
        
        # Check Prometheus
        if curl -s http://localhost:9090/api/v1/status/config > /dev/null 2>&1; then
            print_status "✓ Prometheus is ready"
        else
            print_warning "✗ Prometheus not ready yet"
        fi
        
        # Check Grafana
        if curl -s http://localhost:3000/api/health > /dev/null 2>&1; then
            print_status "✓ Grafana is ready"
        else
            print_warning "✗ Grafana not ready yet"
        fi
        
        # Check Loki
        if curl -s http://localhost:3100/ready > /dev/null 2>&1; then
            print_status "✓ Loki is ready"
        else
            print_warning "✗ Loki not ready yet"
        fi
        
        # Check DNS servers
        local dns_ready=0
        for port in 9092 9093; do
            if curl -s http://localhost:$port/metrics > /dev/null 2>&1; then
                print_status "✓ DNS server on port $port is ready"
                ((dns_ready++))
            else
                print_warning "✗ DNS server on port $port not ready yet"
            fi
        done
        
        if [ $dns_ready -eq 2 ]; then
            print_status "All services are ready!"
            return 0
        fi
        
        sleep 10
        ((attempt++))
    done
    
    print_error "Services did not become ready within expected time"
    return 1
}

# Function to run specific test
run_test() {
    local test_name="$1"
    print_status "Running test: $test_name"
    
    if cargo test --test monitoring_integration_tests "$test_name" -- --nocapture; then
        print_status "✓ Test $test_name PASSED"
        return 0
    else
        print_error "✗ Test $test_name FAILED"
        return 1
    fi
}

# Function to run all monitoring tests
run_all_tests() {
    print_status "Running all monitoring integration tests..."
    
    local tests=(
        "test_metrics_exposure"
        "test_prometheus_scraping"
        "test_end_to_end_monitoring"
        "test_multi_instance_monitoring"
        "test_grafana_dashboard_functionality"
        "test_log_aggregation"
        "test_service_discovery"
        "test_full_monitoring_stack_integration"
    )
    
    local passed=0
    local failed=0
    
    for test in "${tests[@]}"; do
        if run_test "$test"; then
            ((passed++))
        else
            ((failed++))
        fi
        echo ""
    done
    
    print_status "=== Test Summary ==="
    print_status "Passed: $passed"
    if [ $failed -gt 0 ]; then
        print_error "Failed: $failed"
    else
        print_status "Failed: $failed"
    fi
    
    return $failed
}

# Function to stop monitoring stack
stop_monitoring_stack() {
    print_status "Stopping monitoring stack..."
    docker-compose -f docker/docker-compose.monitoring.yml down
    print_status "Monitoring stack stopped"
}

# Function to show logs
show_logs() {
    local service="$1"
    if [ -n "$service" ]; then
        print_status "Showing logs for $service..."
        docker-compose -f docker/docker-compose.monitoring.yml logs "$service"
    else
        print_status "Showing logs for all services..."
        docker-compose -f docker/docker-compose.monitoring.yml logs
    fi
}

# Main script logic
case "${1:-}" in
    "start")
        start_monitoring_stack
        ;;
    "stop")
        stop_monitoring_stack
        ;;
    "status")
        check_monitoring_stack
        ;;
    "test")
        if [ -n "$2" ]; then
            run_test "$2"
        else
            run_all_tests
        fi
        ;;
    "logs")
        show_logs "$2"
        ;;
    "full")
        print_status "Running full monitoring test cycle..."
        
        # Start stack if not running
        if ! check_monitoring_stack; then
            start_monitoring_stack
        fi
        
        # Run tests
        run_all_tests
        test_result=$?
        
        # Optionally stop stack
        if [ "${KEEP_STACK:-}" != "true" ]; then
            stop_monitoring_stack
        fi
        
        exit $test_result
        ;;
    *)
        echo "Usage: $0 {start|stop|status|test [test_name]|logs [service]|full}"
        echo ""
        echo "Commands:"
        echo "  start   - Start the monitoring stack"
        echo "  stop    - Stop the monitoring stack"
        echo "  status  - Check monitoring stack status"
        echo "  test    - Run monitoring integration tests (optionally specify test name)"
        echo "  logs    - Show logs (optionally specify service name)"
        echo "  full    - Run complete test cycle (start, test, stop)"
        echo ""
        echo "Environment variables:"
        echo "  KEEP_STACK=true - Don't stop stack after 'full' command"
        echo ""
        echo "Examples:"
        echo "  $0 start"
        echo "  $0 test test_metrics_exposure"
        echo "  $0 logs prometheus"
        echo "  KEEP_STACK=true $0 full"
        exit 1
        ;;
esac