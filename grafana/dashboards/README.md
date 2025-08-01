# RIND DNS Record Management Dashboard

This directory contains Grafana dashboard configurations for monitoring the RIND DNS server's record management capabilities.

## Dashboard: dns-record-management.json

The **RIND DNS Record Management** dashboard provides comprehensive monitoring of DNS record operations and API performance.

### Dashboard Panels

#### Overview Section
1. **Total Active Records** - Current count of active DNS records in the system
2. **Record Operation Rates** - Real-time rates for create, update, delete operations

#### Performance Monitoring
3. **Record Operation Success Rate** - Success vs error rates for all operations
4. **Record Operation Latency Percentiles** - p50, p95, p99 response times for CRUD operations

#### Error Analysis
5. **Record Operation Error Rates by Type** - Breakdown of errors by operation (create, update, delete, read, list)
6. **Record Operation Error Rates by Error Type** - Breakdown by error category (validation, duplicate, not found, I/O)

#### API Monitoring
7. **API Request Rates by Endpoint** - HTTP request rates for each REST endpoint
8. **API Response Time Percentiles** - p50, p95, p99 response times for API calls
9. **API Error Rates by Endpoint** - Error rates broken down by API endpoint

### Key Metrics

The dashboard monitors these Prometheus metrics:

#### Record Management Metrics
- `active_records_total` - Current number of active DNS records
- `records_created_total` - Total records created
- `records_updated_total` - Total records updated  
- `records_deleted_total` - Total records deleted
- `record_operations_total{operation, status}` - All operations by type and status
- `record_operation_duration_seconds` - Operation latency histogram
- `record_operations_failed_total{operation, error_type}` - Failed operations by error type

#### API Metrics
- `api_requests_total{endpoint, method, status}` - HTTP requests by endpoint and status
- `api_request_duration_seconds{endpoint, method}` - API response time histogram
- `api_errors_total{endpoint, error_type}` - API errors by endpoint and type

### Dashboard Configuration

- **Refresh Rate**: 5 seconds
- **Time Range**: Last 1 hour (configurable)
- **Tags**: dns, rind, record-management
- **UID**: rind-record-management

### Usage

1. Import the dashboard JSON into Grafana
2. Ensure Prometheus is configured to scrape RIND DNS server metrics on port 9090
3. Verify the Prometheus datasource is configured in Grafana
4. The dashboard will automatically populate with metrics data

### Alerting

Alerting rules are configured in `prometheus/record-management-alerts.yml` and include:

- High error rates (>5% for 5 minutes)
- Slow response times (>100ms p95 for 5 minutes)
- I/O errors (critical alert)
- Unusual operation patterns
- Service availability monitoring

### Troubleshooting

If panels show "No data":
1. Verify RIND DNS server is running and exposing metrics on port 9090
2. Check Prometheus is scraping the metrics endpoint
3. Confirm Prometheus datasource is properly configured in Grafana
4. Ensure the metric names match those defined in the RIND server code

For more information, see the main RIND documentation and monitoring setup guide.