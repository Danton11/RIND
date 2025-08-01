#!/bin/bash

# Test script to demonstrate improved dashboard coloring
# Shows how metrics continue even when DNS servers are down

echo "=== Testing Dashboard Color Improvements ==="
echo ""

echo "1. Current canary metrics (servers should be UP):"
curl -s http://localhost:8090/metrics | grep -E "(availability|consecutive_failures|time_since_last_success)" | head -3
echo ""

echo "2. Stopping DNS servers to test error handling..."
./scripts/start-fullstack.sh stop > /dev/null 2>&1
echo "   DNS servers stopped"
echo ""

echo "3. Waiting 15 seconds for canary to detect failures..."
sleep 15

echo "4. Metrics during outage (notice continuous updates):"
curl -s http://localhost:8090/metrics | grep -E "(availability|consecutive_failures|time_since_last_success)" | head -3
echo ""

echo "5. Restarting DNS servers..."
./scripts/start-fullstack.sh start > /dev/null 2>&1
echo "   DNS servers restarted"
echo ""

echo "6. Waiting 10 seconds for recovery..."
sleep 10

echo "7. Metrics after recovery:"
curl -s http://localhost:8090/metrics | grep -E "(availability|consecutive_failures|time_since_last_success)" | head -3
echo ""

echo "=== Dashboard Color Test Complete ==="
echo ""
echo "Key improvements in the new dashboard:"
echo "✅ Total Query Count: Now BLUE (neutral) instead of red"
echo "✅ Total Responses: Now BLUE (neutral) instead of red, renamed to 'Successful Responses'"
echo "✅ Error Rate: Now only counts SERVFAIL/Timeout/Error (not NXDOMAIN)"
echo "✅ NXDOMAIN: Separate BLUE panel showing valid 'not found' responses"
echo "✅ Availability Gauge: Shows DNS server availability with proper thresholds"
echo "✅ Consecutive Failures: Shows current failure streak"
echo "✅ Time Since Success: Shows how long since last successful query"
echo ""
echo "Access the improved dashboard at:"
echo "http://localhost:3000/d/dns-overview-improved/dns-server-overview-improved"
echo "(Login: admin / rind-admin-2025)"