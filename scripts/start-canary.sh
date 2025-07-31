#!/bin/bash

# DNS Canary Startup Script
# Starts the DNS canary in the background and provides control commands

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANARY_SCRIPT="$SCRIPT_DIR/dns-canary.py"

# Parse command line arguments
COMMAND="$1"
DNS_SERVER="${2:-localhost}"
DNS_PORT="${3:-12312}"

# Create unique files based on server to allow multiple canaries
SERVER_SAFE=$(echo "$DNS_SERVER" | sed 's/[^a-zA-Z0-9]/_/g')
PID_FILE="$SCRIPT_DIR/dns-canary-${SERVER_SAFE}.pid"
LOG_FILE="$SCRIPT_DIR/dns-canary-${SERVER_SAFE}.log"

case "$COMMAND" in
    start)
        if [ -f "$PID_FILE" ] && kill -0 $(cat "$PID_FILE") 2>/dev/null; then
            echo "DNS Canary for $DNS_SERVER:$DNS_PORT is already running (PID: $(cat $PID_FILE))"
            exit 1
        fi
        
        echo "Starting DNS Canary targeting $DNS_SERVER:$DNS_PORT..."
        python3 "$CANARY_SCRIPT" --server "$DNS_SERVER" --port "$DNS_PORT" > "$LOG_FILE" 2>&1 &
        echo $! > "$PID_FILE"
        echo "DNS Canary started (PID: $!)"
        echo "Log file: $LOG_FILE"
        ;;
        
    stop)
        if [ -f "$PID_FILE" ]; then
            PID=$(cat "$PID_FILE")
            if kill -0 "$PID" 2>/dev/null; then
                echo "Stopping DNS Canary for $DNS_SERVER:$DNS_PORT (PID: $PID)..."
                kill "$PID"
                rm -f "$PID_FILE"
                echo "DNS Canary stopped"
            else
                echo "DNS Canary for $DNS_SERVER:$DNS_PORT is not running"
                rm -f "$PID_FILE"
            fi
        else
            echo "DNS Canary for $DNS_SERVER:$DNS_PORT is not running"
        fi
        ;;
        
    status)
        if [ -f "$PID_FILE" ] && kill -0 $(cat "$PID_FILE") 2>/dev/null; then
            echo "DNS Canary for $DNS_SERVER:$DNS_PORT is running (PID: $(cat $PID_FILE))"
            echo "Recent log entries:"
            tail -5 "$LOG_FILE" 2>/dev/null || echo "No log file found"
        else
            echo "DNS Canary for $DNS_SERVER:$DNS_PORT is not running"
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
        echo "Usage: $0 {start|stop|status|logs|stats} [DNS_SERVER] [DNS_PORT]"
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
        echo ""
        echo "Examples:"
        echo "  $0 start                    # Target localhost:12312"
        echo "  $0 start dns.example.com    # Target dns.example.com:12312"
        echo "  $0 start 192.168.1.100 53  # Target 192.168.1.100:53"
        echo "  $0 stop localhost           # Stop canary for localhost"
        exit 1
        ;;
esac