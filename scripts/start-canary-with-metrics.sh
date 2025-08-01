#!/bin/bash

# Enhanced DNS Canary Startup Script with Metrics
# Starts the DNS canary with Prometheus metrics endpoint

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANARY_SCRIPT="$SCRIPT_DIR/dns-canary-with-metrics.py"

# Parse command line arguments
COMMAND="$1"
DNS_SERVER="${2:-localhost}"
DNS_PORT="${3:-12312}"
API_SERVER="${4:-localhost}"
API_PORT="${5:-8080}"
METRICS_PORT="${6:-8090}"

# Additional options
NO_PRIME=""
NO_API=""
VERBOSE=""

# Parse additional flags
shift 6 2>/dev/null || true
while [[ $# -gt 0 ]]; do
    case $1 in
        --no-prime)
            NO_PRIME="--no-prime"
            shift
            ;;
        --no-api)
            NO_API="--no-api"
            shift
            ;;
        --verbose|-v)
            VERBOSE="--verbose"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            shift
            ;;
    esac
done

# Create unique files based on server to allow multiple canaries
SERVER_SAFE=$(echo "${DNS_SERVER}_${API_SERVER}_${METRICS_PORT}" | sed 's/[^a-zA-Z0-9]/_/g')
PID_FILE="$SCRIPT_DIR/dns-canary-metrics-${SERVER_SAFE}.pid"
LOG_FILE="$SCRIPT_DIR/dns-canary-metrics-${SERVER_SAFE}.log"

case "$COMMAND" in
    start)
        if [ -f "$PID_FILE" ] && kill -0 $(cat "$PID_FILE") 2>/dev/null; then
            echo "DNS Canary with Metrics is already running (PID: $(cat $PID_FILE))"
            echo "Metrics endpoint: http://localhost:$METRICS_PORT/metrics"
            exit 1
        fi
        
        echo "Starting DNS Canary with Metrics..."
        echo "  DNS Target: $DNS_SERVER:$DNS_PORT"
        echo "  API Target: $API_SERVER:$API_PORT"
        echo "  Metrics Port: $METRICS_PORT"
        
        python3 "$CANARY_SCRIPT" \
            --dns-server "$DNS_SERVER" \
            --dns-port "$DNS_PORT" \
            --api-server "$API_SERVER" \
            --api-port "$API_PORT" \
            --metrics-port "$METRICS_PORT" \
            $NO_PRIME $NO_API $VERBOSE > "$LOG_FILE" 2>&1 &
        echo $! > "$PID_FILE"
        echo "DNS Canary started (PID: $!)"
        echo "Log file: $LOG_FILE"
        echo "Metrics endpoint: http://localhost:$METRICS_PORT/metrics"
        echo "Health endpoint: http://localhost:$METRICS_PORT/health"
        ;;
        
    stop)
        if [ -f "$PID_FILE" ]; then
            PID=$(cat "$PID_FILE")
            if kill -0 "$PID" 2>/dev/null; then
                echo "Stopping DNS Canary with Metrics (PID: $PID)..."
                kill "$PID"
                rm -f "$PID_FILE"
                echo "DNS Canary stopped"
            else
                echo "DNS Canary is not running"
                rm -f "$PID_FILE"
            fi
        else
            echo "DNS Canary is not running"
        fi
        ;;
        
    status)
        if [ -f "$PID_FILE" ] && kill -0 $(cat "$PID_FILE") 2>/dev/null; then
            echo "DNS Canary with Metrics is running (PID: $(cat $PID_FILE))"
            echo "Metrics endpoint: http://localhost:$METRICS_PORT/metrics"
            echo "Health endpoint: http://localhost:$METRICS_PORT/health"
            echo ""
            echo "Recent log entries:"
            tail -5 "$LOG_FILE" 2>/dev/null || echo "No log file found"
        else
            echo "DNS Canary with Metrics is not running"
        fi
        ;;
        
    logs)
        if [ -f "$LOG_FILE" ]; then
            tail -f "$LOG_FILE"
        else
            echo "No log file found"
        fi
        ;;
        
    metrics)
        if [ -f "$PID_FILE" ] && kill -0 $(cat "$PID_FILE") 2>/dev/null; then
            echo "Fetching current metrics from http://localhost:$METRICS_PORT/metrics"
            echo ""
            curl -s "http://localhost:$METRICS_PORT/metrics" || echo "Failed to fetch metrics"
        else
            echo "DNS Canary with Metrics is not running"
        fi
        ;;
        
    *)
        echo "Usage: $0 {start|stop|status|logs|metrics} [DNS_SERVER] [DNS_PORT] [API_SERVER] [API_PORT] [METRICS_PORT] [OPTIONS]"
        echo ""
        echo "Commands:"
        echo "  start   - Start the DNS canary with metrics endpoint"
        echo "  stop    - Stop the DNS canary"
        echo "  status  - Check if canary is running"
        echo "  logs    - Follow the canary logs"
        echo "  metrics - Show current Prometheus metrics"
        echo ""
        echo "Parameters:"
        echo "  DNS_SERVER    - Target DNS server hostname/IP (default: localhost)"
        echo "  DNS_PORT      - Target DNS server port (default: 12312)"
        echo "  API_SERVER    - Target API server hostname/IP (default: localhost)"
        echo "  API_PORT      - Target API server port (default: 8080)"
        echo "  METRICS_PORT  - Metrics server port (default: 8090)"
        echo ""
        echo "Options:"
        echo "  --no-prime   - Skip datastore priming with sample records"
        echo "  --no-api     - Disable API operations (DNS queries only)"
        echo "  --verbose    - Enable verbose logging"
        echo ""
        echo "Examples:"
        echo "  $0 start                                           # Default settings"
        echo "  $0 start localhost 12312 localhost 8080 8090      # Explicit ports"
        echo "  $0 start localhost 12312 localhost 8080 8090 --verbose  # With verbose logging"
        echo "  $0 metrics                                         # Show current metrics"
        echo ""
        echo "Metrics will be available at: http://localhost:[METRICS_PORT]/metrics"
        echo "Health check available at: http://localhost:[METRICS_PORT]/health"
        exit 1
        ;;
esac