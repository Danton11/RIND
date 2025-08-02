#!/bin/bash

# RIND DNS Canary Management Script

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANARY_SCRIPT="$SCRIPT_DIR/dns-canary.py"

# Parse command line arguments
COMMAND="start"
DNS_SERVER="localhost"
DNS_PORT="12312"
API_SERVER="localhost"
API_PORT="8080"
METRICS_PORT="8090"
LOG_LEVEL="INFO"
DAEMON_MODE=false

# Function to show help
show_help() {
    echo "RIND DNS Canary Management Script"
    echo ""
    echo "Usage: $0 [COMMAND] [OPTIONS]"
    echo ""
    echo "Commands:"
    echo "  start                 Start the canary (default)"
    echo "  stop                  Stop the canary daemon"
    echo "  restart               Restart the canary daemon"
    echo "  status                Show canary status"
    echo ""
    echo "Options:"
    echo "  --dns-server HOST     DNS server hostname (default: localhost)"
    echo "  --dns-port PORT       DNS server port (default: 12312)"
    echo "  --api-server HOST     API server hostname (default: localhost)"
    echo "  --api-port PORT       API server port (default: 8080)"
    echo "  --metrics-port PORT   Metrics server port (default: 8090)"
    echo "  --log-level LEVEL     Log level (default: INFO)"
    echo "  --daemon              Run in daemon mode (for start command)"
    echo "  --help                Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 start                              # Start with defaults"
    echo "  $0 start --daemon                     # Start in daemon mode"
    echo "  $0 stop                               # Stop daemon"
    echo "  $0 restart --metrics-port 8091       # Restart with different port"
    echo "  $0 status                             # Check status"
    echo ""
}

# Function to get PID file path
get_pid_file() {
    echo "$SCRIPT_DIR/dns-canary-${DNS_SERVER}_${API_SERVER}_${METRICS_PORT}.pid"
}

# Function to get log file path
get_log_file() {
    echo "$SCRIPT_DIR/dns-canary-${DNS_SERVER}_${API_SERVER}_${METRICS_PORT}.log"
}

# Function to check if canary is running
is_running() {
    local pid_file=$(get_pid_file)
    if [ -f "$pid_file" ]; then
        local pid=$(cat "$pid_file")
        if kill -0 "$pid" 2>/dev/null; then
            return 0
        else
            # PID file exists but process is dead, clean up
            rm -f "$pid_file"
            return 1
        fi
    fi
    return 1
}

# Function to stop canary
stop_canary() {
    local pid_file=$(get_pid_file)
    
    if is_running; then
        local pid=$(cat "$pid_file")
        echo "Stopping canary (PID: $pid)..."
        
        if kill "$pid" 2>/dev/null; then
            # Wait for process to stop
            local count=0
            while kill -0 "$pid" 2>/dev/null && [ $count -lt 10 ]; do
                sleep 1
                count=$((count + 1))
            done
            
            if kill -0 "$pid" 2>/dev/null; then
                echo "Process didn't stop gracefully, force killing..."
                kill -9 "$pid" 2>/dev/null
            fi
            
            rm -f "$pid_file"
            echo "Canary stopped successfully"
        else
            echo "Failed to stop canary process"
            return 1
        fi
    else
        echo "Canary is not running"
        return 1
    fi
}

# Function to show status
show_status() {
    local pid_file=$(get_pid_file)
    local log_file=$(get_log_file)
    
    echo "RIND DNS Canary Status"
    echo "======================"
    echo "Configuration:"
    echo "  DNS Server: $DNS_SERVER:$DNS_PORT"
    echo "  API Server: $API_SERVER:$API_PORT"
    echo "  Metrics Port: $METRICS_PORT"
    echo "  PID File: $pid_file"
    echo "  Log File: $log_file"
    echo ""
    
    if is_running; then
        local pid=$(cat "$pid_file")
        echo "Status: RUNNING (PID: $pid)"
        echo "Metrics: http://localhost:$METRICS_PORT/metrics"
        echo "Health: http://localhost:$METRICS_PORT/health"
        
        # Show recent log entries if log file exists
        if [ -f "$log_file" ]; then
            echo ""
            echo "Recent log entries:"
            tail -5 "$log_file" 2>/dev/null || echo "  (no recent entries)"
        fi
    else
        echo "Status: NOT RUNNING"
    fi
}

while [[ $# -gt 0 ]]; do
    case $1 in
        start|stop|restart|status)
            COMMAND="$1"
            shift
            ;;
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
            show_help
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Function to start canary
start_canary() {
    # Check if already running
    if is_running; then
        echo "Canary is already running (PID: $(cat $(get_pid_file)))"
        echo "Use 'stop' command first, or 'restart' to restart"
        return 1
    fi
    
    # Check if Python script exists
    if [ ! -f "$CANARY_SCRIPT" ]; then
        echo "Error: Canary script not found at $CANARY_SCRIPT"
        exit 1
    fi
    
    # Make script executable
    chmod +x "$CANARY_SCRIPT"
    
    local log_file=$(get_log_file)
    local pid_file=$(get_pid_file)
    
    echo "Starting RIND DNS Canary..."
    echo "DNS Server: $DNS_SERVER:$DNS_PORT"
    echo "API Server: $API_SERVER:$API_PORT"
    echo "Metrics Port: $METRICS_PORT"
    echo "Log Level: $LOG_LEVEL"
    
    if [ "$DAEMON_MODE" = true ]; then
        echo "Running in daemon mode"
        echo "Log file: $log_file"
        echo "PID file: $pid_file"
        
        # Start in background
        nohup python3 "$CANARY_SCRIPT" \
            --dns-server "$DNS_SERVER" \
            --dns-port "$DNS_PORT" \
            --api-server "$API_SERVER" \
            --api-port "$API_PORT" \
            --metrics-port "$METRICS_PORT" \
            --log-level "$LOG_LEVEL" \
            > "$log_file" 2>&1 &
        
        # Save PID
        echo $! > "$pid_file"
        echo "Canary started with PID $(cat $pid_file)"
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
}

# Main execution
case $COMMAND in
    start)
        start_canary
        ;;
    stop)
        stop_canary
        ;;
    restart)
        echo "Restarting canary..."
        stop_canary
        sleep 2
        DAEMON_MODE=true  # Force daemon mode for restart
        start_canary
        ;;
    status)
        show_status
        ;;
    *)
        echo "Unknown command: $COMMAND"
        echo "Use --help for usage information"
        exit 1
        ;;
esac