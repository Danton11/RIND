#!/bin/bash

# RIND DNS Canary Startup Script

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANARY_SCRIPT="$SCRIPT_DIR/dns-canary.py"

# Parse command line arguments
DNS_SERVER="localhost"
DNS_PORT="12312"
API_SERVER="localhost"
API_PORT="8080"
METRICS_PORT="8090"
LOG_LEVEL="INFO"
DAEMON_MODE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --dns-server)
            DNS_SERVER="$2"
            shift 2
            ;;
        --dns-port)
            DNS_PORT="$2"
            shift 2
            ;;
        --api-server)
            API_SERVER="$2"
            shift 2
            ;;
        --api-port)
            API_PORT="$2"
            shift 2
            ;;
        --metrics-port)
            METRICS_PORT="$2"
            shift 2
            ;;
        --log-level)
            LOG_LEVEL="$2"
            shift 2
            ;;
        --daemon)
            DAEMON_MODE=true
            shift
            ;;
        --help)
            echo "RIND DNS Canary Startup Script"
            echo ""
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --dns-server HOST     DNS server hostname (default: localhost)"
            echo "  --dns-port PORT       DNS server port (default: 12312)"
            echo "  --api-server HOST     API server hostname (default: localhost)"
            echo "  --api-port PORT       API server port (default: 8080)"
            echo "  --metrics-port PORT   Metrics server port (default: 8090)"
            echo "  --log-level LEVEL     Log level (default: INFO)"
            echo "  --daemon              Run in daemon mode"
            echo "  --help                Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0                                    # Start with defaults"
            echo "  $0 --dns-server 192.168.1.100        # Monitor remote DNS server"
            echo "  $0 --metrics-port 8091 --daemon      # Run on different port in background"
            echo ""
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Check if Python script exists
if [ ! -f "$CANARY_SCRIPT" ]; then
    echo "Error: Canary script not found at $CANARY_SCRIPT"
    exit 1
fi

# Make script executable
chmod +x "$CANARY_SCRIPT"

# Create log file name
LOG_FILE="$SCRIPT_DIR/dns-canary-${DNS_SERVER}_${API_SERVER}_${METRICS_PORT}.log"
PID_FILE="$SCRIPT_DIR/dns-canary-${DNS_SERVER}_${API_SERVER}_${METRICS_PORT}.pid"

echo "Starting RIND DNS Canary..."
echo "DNS Server: $DNS_SERVER:$DNS_PORT"
echo "API Server: $API_SERVER:$API_PORT"
echo "Metrics Port: $METRICS_PORT"
echo "Log Level: $LOG_LEVEL"

if [ "$DAEMON_MODE" = true ]; then
    echo "Running in daemon mode"
    echo "Log file: $LOG_FILE"
    echo "PID file: $PID_FILE"
    
    # Start in background
    nohup python3 "$CANARY_SCRIPT" \
        --dns-server "$DNS_SERVER" \
        --dns-port "$DNS_PORT" \
        --api-server "$API_SERVER" \
        --api-port "$API_PORT" \
        --metrics-port "$METRICS_PORT" \
        --log-level "$LOG_LEVEL" \
        > "$LOG_FILE" 2>&1 &
    
    # Save PID
    echo $! > "$PID_FILE"
    echo "Canary started with PID $(cat $PID_FILE)"
    echo "Metrics available at: http://localhost:$METRICS_PORT/metrics"
    echo "Health check at: http://localhost:$METRICS_PORT/health"
else
    # Run in foreground
    echo "Running in foreground mode (Ctrl+C to stop)"
    echo "Metrics available at: http://localhost:$METRICS_PORT/metrics"
    echo "Health check at: http://localhost:$METRICS_PORT/health"
    echo ""
    
    exec python3 "$CANARY_SCRIPT" \
        --dns-server "$DNS_SERVER" \
        --dns-port "$DNS_PORT" \
        --api-server "$API_SERVER" \
        --api-port "$API_PORT" \
        --metrics-port "$METRICS_PORT" \
        --log-level "$LOG_LEVEL"
fi