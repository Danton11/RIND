# RIND DNS Server - Grafana Metrics Guide

## 🚀 Quick Access

**Grafana URL**: http://localhost:3000  
**Username**: admin  
**Password**: rind-admin-2025

## 📊 Available Metrics

### Core DNS Metrics
- `active_records_total` - Current number of DNS records
- `dns_queries_total` - Total DNS queries processed
- `dns_queries_per_second` - Current query rate
- `dns_packet_errors_total` - DNS packet processing errors
- `dns_nxdomain_total` - NXDOMAIN responses
- `dns_active_connections` - Active DNS connections

### API Metrics
- `api_requests_total` - Total API requests by endpoint
- `api_request_duration_seconds` - API request latency histogram
- `api_request_duration_seconds_bucket` - Latency buckets
- `api_request_duration_seconds_count` - Request count
- `api_request_duration_seconds_sum` - Total request time

## 🔍 Using Grafana Explore

1. **Navigate to Explore**
   - Click the compass icon (🧭) in the left sidebar
   - Or go to: http://localhost:3000/explore

2. **Select Data Source**
   - Choose "Prometheus" from the dropdown (should be default)

3. **Try These Sample Queries**
   ```promql
   # Current active records
   active_records_total
   
   # DNS query rate over time
   rate(dns_queries_total[5m])
   
   # API requests by endpoint
   sum by (endpoint) (api_requests_total)
   
   # DNS query latency percentiles
   histogram_quantile(0.95, rate(dns_query_duration_seconds_bucket[5m]))
   
   # Error rate
   rate(dns_packet_errors_total[5m])
   ```

## 📈 Pre-built Dashboards

The following dashboards are automatically provisioned:

- **DNS Overview** - High-level DNS server metrics
- **DNS Error Analysis** - Error tracking and analysis
- **DNS Protocol** - Protocol-level statistics
- **DNS Record Management** - Record CRUD operations
- **Simple Test Dashboard** - Basic metrics for testing

## 🛠 Troubleshooting

### No Metrics Showing?

1. **Check Prometheus Targets**
   ```bash
   curl http://localhost:9090/api/v1/targets
   ```
   Both `dns-server-primary` and `dns-server-secondary` should show as "up"

2. **Verify Metrics Endpoint**
   ```bash
   curl http://localhost:9091/metrics | head -20
   curl http://localhost:9092/metrics | head -20
   ```

3. **Test Direct Prometheus Query**
   ```bash
   curl 'http://localhost:9090/api/v1/query?query=active_records_total'
   ```

### Grafana Can't Connect to Prometheus?

1. **Check Datasource Configuration**
   - Go to Configuration → Data Sources
   - Verify Prometheus URL is `http://prometheus:9090`
   - Test the connection

2. **Restart Services if Needed**
   ```bash
   ./scripts/start-fullstack.sh restart --production
   ```

### Dashboard Not Loading?

1. **Check Dashboard Provisioning**
   ```bash
   docker logs rind-grafana | grep -i dashboard
   ```

2. **Manually Import Dashboard**
   - Go to + → Import
   - Upload one of the JSON files from `monitoring/grafana/dashboards/`

## 📝 Creating Custom Dashboards

1. **Start with Explore**
   - Test your queries in Explore first
   - Use the "Add to Dashboard" button

2. **Common Panel Types**
   - **Stat**: Single value metrics (e.g., `active_records_total`)
   - **Time Series**: Metrics over time (e.g., `rate(dns_queries_total[5m])`)
   - **Gauge**: Percentage/ratio metrics
   - **Table**: Multi-dimensional data

3. **Useful PromQL Functions**
   - `rate()` - Per-second rate of increase
   - `sum()` - Sum across dimensions
   - `histogram_quantile()` - Percentiles from histograms
   - `increase()` - Total increase over time range

## 🎯 Key Performance Indicators

Monitor these metrics for DNS server health:

1. **Query Rate**: `rate(dns_queries_total[5m])`
2. **Error Rate**: `rate(dns_packet_errors_total[5m])`
3. **Response Time**: `histogram_quantile(0.95, rate(dns_query_duration_seconds_bucket[5m]))`
4. **Active Records**: `active_records_total`
5. **API Health**: `rate(api_requests_total[5m])`

## 🔗 Useful Links

- **Prometheus**: http://localhost:9090
- **Grafana Explore**: http://localhost:3000/explore
- **HAProxy Stats**: http://localhost:8404/stats
- **DNS API**: http://localhost:80/records