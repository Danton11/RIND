#!/bin/bash

# DNS Canary Startup Script
# Starts the DNS canary in the background and provides control commands

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANARY_SCRIPT="$SCRIPT_DIR/dns-canary.py"

# Parse command line arguments
COMMAND="$1"
DNS_SERVER="${2:-localhost}"
DNS_PORT="${3:-12312}"
API_SERVER="${4:-localhost}"
API_PORT="${5:-8080}"

# Additional options
NO_PRIME=""
NO_API=""
NO_CLEANUP=""
VERBOSE=""

# Parse additional flags
shift 5 2>/dev/null || true
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
        --no-cleanup)
            NO_CLEANUP="--no-cleanup"
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
SERVER_SAFE=$(echo "${DNS_SERVER}_${API_SERVER}" | sed 's/[^a-zA-Z0-9]/_/g')
PID_FILE="$SCRIPT_DIR/dns-canary-${SERVER_SAFE}.pid"
LOG_FILE="$SCRIPT_DIR/dns-canary-${SERVER_SAFE}.log"

case "$COMMAND" in
    start)
        if [ -f "$PID_FILE" ] && kill -0 $(cat "$PID_FILE") 2>/dev/null; then
            echo "DNS Canary for DNS:$DNS_SERVER:$DNS_PORT API:$API_SERVER:$API_PORT is already running (PID: $(cat $PID_FILE))"
            exit 1
        fi
        
        echo "Starting DNS Canary targeting DNS:$DNS_SERVER:$DNS_PORT API:$API_SERVER:$API_PORT..."
        python3 "$CANARY_SCRIPT" \
            --dns-server "$DNS_SERVER" \
            --dns-port "$DNS_PORT" \
            --api-server "$API_SERVER" \
            --api-port "$API_PORT" \
            $NO_PRIME $NO_API $NO_CLEANUP $VERBOSE > "$LOG_FILE" 2>&1 &
        echo $! > "$PID_FILE"
        echo "DNS Canary started (PID: $!)"
        echo "Log file: $LOG_FILE"
        ;;
        
    stop)
        if [ -f "$PID_FILE" ]; then
            PID=$(cat "$PID_FILE")
            if kill -0 "$PID" 2>/dev/null; then
                echo "Stopping DNS Canary for DNS:$DNS_SERVER:$DNS_PORT API:$API_SERVER:$API_PORT (PID: $PID)..."
                kill "$PID"
                rm -f "$PID_FILE"
                echo "DNS Canary stopped"
            else
                echo "DNS Canary for DNS:$DNS_SERVER:$DNS_PORT API:$API_SERVER:$API_PORT is not running"
                rm -f "$PID_FILE"
            fi
        else
            echo "DNS Canary for DNS:$DNS_SERVER:$DNS_PORT API:$API_SERVER:$API_PORT is not running"
        fi
        ;;
        
    status)
        if [ -f "$PID_FILE" ] && kill -0 $(cat "$PID_FILE") 2>/dev/null; then
            echo "DNS Canary for DNS:$DNS_SERVER:$DNS_PORT API:$API_SERVER:$API_PORT is running (PID: $(cat $PID_FILE))"
            echo "Recent log entries:"
            tail -5 "$LOG_FILE" 2>/dev/null || echo "No log file found"
        else
            echo "DNS Canary for DNS:$DNS_SERVER:$DNS_PORT API:$API_SERVER:$API_PORT is not running"
        fi
        ;;
        
    logs)
        if [ -f "$LOG_FILE" ]; then
            tail -f "$LOG_FILE"
        else
            echo "No log file found"
        fi
        ;;
        
    stats)
        if [ -f "$LOG_FILE" ]; then
            echo "Recent statistics:"
            grep "Stats -" "$LOG_FILE" | tail -5
        else
            echo "No log file found"
        fi
        ;;
        
    *)
        echo "Usage: $0 {start|stop|status|logs|stats} [DNS_SERVER] [DNS_PORT] [API_SERVER] [API_PORT] [OPTIONS]"
        echo ""
        echo "Commands:"
        echo "  start  - Start the DNS canary in background"
        echo "  stop   - Stop the DNS canary"
        echo "  status - Check if canary is running"
        echo "  logs   - Follow the canary logs"
        echo "  stats  - Show recent statistics"
        echo ""
        echo "Parameters:"
        echo "  DNS_SERVER - Target DNS server hostname/IP (default: localhost)"
        echo "  DNS_PORT   - Target DNS server port (default: 12312)"
        echo "  API_SERVER - Target API server hostname/IP (default: localhost)"
        echo "  API_PORT   - Target API server port (default: 8080)"
        echo ""
        echo "Options:"
        echo "  --no-prime   - Skip datastore priming with sample records"
        echo "  --no-api     - Disable API operations (DNS queries only)"
        echo "  --no-cleanup - Preserve created records on exit (don't cleanup)"
        echo "  --verbose    - Enable verbose logging"
        echo ""
        echo "Examples:"
        echo "  $0 start                                    # Target localhost:12312 and localhost:8080"
        echo "  $0 start dns.example.com                   # Target dns.example.com:12312 and localhost:8080"
        echo "  $0 start localhost 12312 api.example.com   # Target localhost:12312 and api.example.com:8080"
        echo "  $0 start localhost 12312 localhost 8080 --verbose    # With verbose logging"
        echo "  $0 start localhost 12312 localhost 8080 --no-api     # DNS queries only"
        echo "  $0 start localhost 12312 localhost 8080 --no-cleanup # Preserve test records"
        echo "  $0 stop localhost                          # Stop canary for localhost"
        exit 1
        ;;
esac