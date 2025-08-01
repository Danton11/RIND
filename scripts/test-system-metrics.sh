#!/bin/bash

# RIND System Metrics Testing Script
# Tests the system metrics exporter and dashboard functionality

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
METRICS_URL="${METRICS_URL:-http://localhost:8091}"
PROMETHEUS_URL="${PROMETHEUS_URL:-http://localhost:9090}"
GRAFANA_URL="${GRAFANA_URL:-http://localhost:3000}"

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

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

# Function to test HTTP endpoint
test_endpoint() {
    local url="$1"
    local description="$2"
    local expected_status="${3:-200}"
    
    log_info "Testing $description: $url"
    
    local response
    local status_code
    
    if response=$(curl -s -w "%{http_code}" "$url" 2>/dev/null); then
        status_code="${response: -3}"
        response="${response%???}"
        
        if [ "$status_code" = "$expected_status" ]; then
            log_success "$description is responding (HTTP $status_code)"
            return 0
        else
            log_error "$description returned HTTP $status_code (expected $expected_status)"
            return 1
        fi
    else
        log_error "$description is not accessible"
        return 1
    fi
}

# Function to test metrics content
test_metrics_content() {
    local url="$METRICS_URL/metrics"
    
    log_info "Testing metrics content..."
    
    local response
    if response=$(curl -s "$url" 2>/dev/null); then
        # Check for expected metrics
        local expected_metrics=(
            "rind_process_cpu_percent"
            "rind_process_memory_rss_bytes"
            "rind_docker_containers_total"
            "rind_dns_connections_total"
            "rind_system_file_descriptors_allocated"
            "rind_dns_records_count"
            "rind_metrics_exporter_uptime_seconds"
        )
        
        local missing_metrics=()
        for metric in "${expected_metrics[@]}"; do
            if ! echo "$response" | grep -q "$metric"; then
                missing_metrics+=("$metric")
            fi
        done
        
        if [ ${#missing_metrics[@]} -eq 0 ]; then
            log_success "All expected metrics are present"
            
            # Show sample metrics
            log_info "Sample metrics:"
            echo "$response" | grep -E "^rind_" | head -10 | while read -r line; do
                echo "  $line"
            done
            
            return 0
        else
            log_error "Missing metrics: ${missing_metrics[*]}"
            return 1
        fi
    else
        log_error "Failed to fetch metrics content"
        return 1
    fi
}

# Function to test Prometheus scraping
test_prometheus_scraping() {
    local prometheus_targets_url="$PROMETHEUS_URL/api/v1/targets"
    
    log_info "Testing Prometheus target scraping..."
    
    local response
    if response=$(curl -s "$prometheus_targets_url" 2>/dev/null); then
        if echo "$response" | jq -e '.data.activeTargets[] | select(.labels.job == "rind-system-metrics")' >/dev/null 2>&1; then
            local target_health
            target_health=$(echo "$response" | jq -r '.data.activeTargets[] | select(.labels.job == "rind-system-metrics") | .health')
            
            if [ "$target_health" = "up" ]; then
                log_success "Prometheus is successfully scraping system metrics"
                return 0
            else
                log_error "Prometheus target is down: $target_health"
                return 1
            fi
        else
            log_error "System metrics target not found in Prometheus"
            return 1
        fi
    else
        log_error "Failed to query Prometheus targets"
        return 1
    fi
}

# Function to test specific metrics queries
test_metrics_queries() {
    local prometheus_query_url="$PROMETHEUS_URL/api/v1/query"
    
    log_info "Testing specific metrics queries..."
    
    local test_queries=(
        "rind_process_cpu_percent"
        "rind_process_memory_rss_bytes"
        "rind_dns_connections_total"
        "rind_system_file_descriptors_usage_percent"
    )
    
    local failed_queries=()
    for query in "${test_queries[@]}"; do
        local response
        if response=$(curl -s "$prometheus_query_url?query=$query" 2>/dev/null); then
            if echo "$response" | jq -e '.data.result | length > 0' >/dev/null 2>&1; then
                local value
                value=$(echo "$response" | jq -r '.data.result[0].value[1]' 2>/dev/null || echo "N/A")
                log_success "Query '$query' returned value: $value"
            else
                failed_queries+=("$query")
            fi
        else
            failed_queries+=("$query")
        fi
    done
    
    if [ ${#failed_queries[@]} -eq 0 ]; then
        log_success "All metric queries successful"
        return 0
    else
        log_error "Failed queries: ${failed_queries[*]}"
        return 1
    fi
}

# Function to test dashboard availability
test_dashboard() {
    local dashboard_url="$GRAFANA_URL/api/dashboards/uid/rind-system-metrics"
    
    log_info "Testing system metrics dashboard..."
    
    local response
    if response=$(curl -s -u admin:rind-admin-2025 "$dashboard_url" 2>/dev/null); then
        if echo "$response" | jq -e '.dashboard.title' >/dev/null 2>&1; then
            local title
            title=$(echo "$response" | jq -r '.dashboard.title')
            log_success "Dashboard found: $title"
            return 0
        else
            log_error "Dashboard response invalid"
            return 1
        fi
    else
        log_error "Dashboard not accessible"
        return 1
    fi
}

# Function to show system metrics summary
show_metrics_summary() {
    log_info "System Metrics Summary"
    log_info "====================="
    
    local metrics_response
    if metrics_response=$(curl -s "$METRICS_URL/metrics" 2>/dev/null); then
        # Extract key metrics
        local cpu_usage
        local memory_rss
        local connections
        local records_count
        local uptime
        
        cpu_usage=$(echo "$metrics_response" | grep "rind_process_cpu_percent" | head -1 | awk '{print $2}' || echo "N/A")
        memory_rss=$(echo "$metrics_response" | grep "rind_process_memory_rss_bytes" | head -1 | awk '{print $2}' || echo "N/A")
        connections=$(echo "$metrics_response" | grep "rind_dns_connections_total" | awk '{print $2}' || echo "N/A")
        records_count=$(echo "$metrics_response" | grep "rind_dns_records_count" | awk '{print $2}' || echo "N/A")
        uptime=$(echo "$metrics_response" | grep "rind_metrics_exporter_uptime_seconds" | awk '{print $2}' || echo "N/A")
        
        echo "  Process CPU Usage: ${cpu_usage}%"
        echo "  Process Memory (RSS): ${memory_rss} bytes"
        echo "  DNS Connections: ${connections}"
        echo "  DNS Records Count: ${records_count}"
        echo "  Exporter Uptime: ${uptime} seconds"
    else
        log_error "Failed to fetch metrics summary"
    fi
}

# Function to run all tests
run_all_tests() {
    log_info "RIND System Metrics Test Suite"
    log_info "=============================="
    
    local tests_passed=0
    local tests_total=0
    
    # Test system metrics exporter
    ((tests_total++))
    if test_endpoint "$METRICS_URL/health" "System Metrics Health Check"; then
        ((tests_passed++))
    fi
    
    ((tests_total++))
    if test_endpoint "$METRICS_URL/metrics" "System Metrics Endpoint"; then
        ((tests_passed++))
    fi
    
    ((tests_total++))
    if test_metrics_content; then
        ((tests_passed++))
    fi
    
    # Test Prometheus integration
    ((tests_total++))
    if test_endpoint "$PROMETHEUS_URL/-/healthy" "Prometheus Health Check"; then
        ((tests_passed++))
    fi
    
    ((tests_total++))
    if test_prometheus_scraping; then
        ((tests_passed++))
    fi
    
    ((tests_total++))
    if test_metrics_queries; then
        ((tests_passed++))
    fi
    
    # Test Grafana dashboard
    ((tests_total++))
    if test_endpoint "$GRAFANA_URL/api/health" "Grafana Health Check"; then
        ((tests_passed++))
    fi
    
    ((tests_total++))
    if test_dashboard; then
        ((tests_passed++))
    fi
    
    # Show summary
    echo
    log_info "Test Results: $tests_passed/$tests_total tests passed"
    
    if [ $tests_passed -eq $tests_total ]; then
        log_success "All tests passed! System metrics monitoring is working correctly."
        show_metrics_summary
        
        echo
        log_info "Access URLs:"
        echo "  System Metrics: $METRICS_URL/metrics"
        echo "  Prometheus: $PROMETHEUS_URL"
        echo "  Grafana Dashboard: $GRAFANA_URL/d/rind-system-metrics"
        
        return 0
    else
        log_error "Some tests failed. Check the system metrics setup."
        return 1
    fi
}

# Function to show usage
show_usage() {
    cat << EOF
RIND System Metrics Testing Script

Usage: $0 [OPTIONS] [COMMAND]

Commands:
    test            Run all tests (default)
    health          Test health endpoints only
    metrics         Test metrics content only
    prometheus      Test Prometheus integration only
    grafana         Test Grafana dashboard only
    summary         Show current metrics summary

Options:
    -h, --help              Show this help message
    -m, --metrics-url URL   System metrics URL (default: http://localhost:8091)
    -p, --prometheus-url URL Prometheus URL (default: http://localhost:9090)
    -g, --grafana-url URL   Grafana URL (default: http://localhost:3000)

Environment Variables:
    METRICS_URL     System metrics exporter URL
    PROMETHEUS_URL  Prometheus server URL
    GRAFANA_URL     Grafana server URL

Examples:
    $0                                      # Run all tests
    $0 health                              # Test health endpoints only
    $0 -m http://localhost:8091 metrics    # Test specific metrics URL
    $0 summary                             # Show metrics summary

EOF
}

# Parse command line arguments
COMMAND="test"
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_usage
            exit 0
            ;;
        -m|--metrics-url)
            METRICS_URL="$2"
            shift 2
            ;;
        -p|--prometheus-url)
            PROMETHEUS_URL="$2"
            shift 2
            ;;
        -g|--grafana-url)
            GRAFANA_URL="$2"
            shift 2
            ;;
        test|health|metrics|prometheus|grafana|summary)
            COMMAND="$1"
            shift
            ;;
        *)
            log_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# Execute command
case $COMMAND in
    test)
        run_all_tests
        ;;
    health)
        test_endpoint "$METRICS_URL/health" "System Metrics Health Check"
        test_endpoint "$PROMETHEUS_URL/-/healthy" "Prometheus Health Check"
        test_endpoint "$GRAFANA_URL/api/health" "Grafana Health Check"
        ;;
    metrics)
        test_endpoint "$METRICS_URL/metrics" "System Metrics Endpoint"
        test_metrics_content
        ;;
    prometheus)
        test_prometheus_scraping
        test_metrics_queries
        ;;
    grafana)
        test_dashboard
        ;;
    summary)
        show_metrics_summary
        ;;
    *)
        log_error "Unknown command: $COMMAND"
        show_usage
        exit 1
        ;;
esac