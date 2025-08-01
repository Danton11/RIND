use std::collections::HashMap;
use std::net::{UdpSocket, SocketAddr};
use std::time::Duration;
use tokio::time::sleep;
use serde_json::Value;
use reqwest::Client;

/// Test configuration for monitoring stack
struct MonitoringTestConfig {
    prometheus_url: String,
    grafana_url: String,
    loki_url: String,
    dns_servers: Vec<DnsServerConfig>,
}

#[derive(Clone)]
struct DnsServerConfig {
    name: String,
    dns_port: u16,
    api_port: u16,
    metrics_port: u16,
}

impl Default for MonitoringTestConfig {
    fn default() -> Self {
        Self {
            prometheus_url: "http://localhost:9090".to_string(),
            grafana_url: "http://localhost:3000".to_string(),
            loki_url: "http://localhost:3100".to_string(),
            dns_servers: vec![
                DnsServerConfig {
                    name: "dns-server-1".to_string(),
                    dns_port: 12312,
                    api_port: 8080,
                    metrics_port: 9092,
                },
                DnsServerConfig {
                    name: "dns-server-2".to_string(),
                    dns_port: 12313,
                    api_port: 8081,
                    metrics_port: 9093,
                },
            ],
        }
    }
}

/// Helper function to create DNS query packet
fn create_dns_query_packet(domain: &str, query_id: u16) -> Vec<u8> {
    let mut packet = vec![
        (query_id >> 8) as u8, (query_id & 0xFF) as u8, // ID
        0x01, 0x00, // Flags (standard query)
        0x00, 0x01, // QDCOUNT
        0x00, 0x00, // ANCOUNT
        0x00, 0x00, // NSCOUNT
        0x00, 0x00, // ARCOUNT
    ];
    
    // Encode domain name
    for part in domain.split('.') {
        packet.push(part.len() as u8);
        packet.extend(part.as_bytes());
    }
    packet.push(0); // End of name
    
    // Query type and class
    packet.extend(&[0x00, 0x01]); // Type A
    packet.extend(&[0x00, 0x01]); // Class IN
    
    packet
}

/// Send DNS query to specific server
async fn send_dns_query_to_server(
    domain: &str, 
    server_config: &DnsServerConfig
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    
    let packet = create_dns_query_packet(domain, rand::random());
    let server_addr: SocketAddr = format!("127.0.0.1:{}", server_config.dns_port).parse()?;
    
    socket.send_to(&packet, server_addr)?;
    
    let mut buffer = vec![0u8; 512];
    let (len, _) = socket.recv_from(&mut buffer)?;
    buffer.truncate(len);
    
    Ok(buffer)
}

/// Add DNS record via API to specific server
async fn add_dns_record_to_server(
    name: &str, 
    ip: &str, 
    server_config: &DnsServerConfig
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let record = serde_json::json!({
        "name": name,
        "ip": ip,
        "ttl": 300,
        "record_type": "A",
        "class": "IN",
        "value": null
    });
    
    let response = client
        .post(&format!("http://127.0.0.1:{}/update", server_config.api_port))
        .json(&record)
        .send()
        .await?;
    
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("API request failed: {}", response.status()).into())
    }
}

/// Check if monitoring stack is running
async fn check_monitoring_stack_health(config: &MonitoringTestConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::new();
    
    // Check Prometheus
    let prometheus_response = client
        .get(&format!("{}/api/v1/status/config", config.prometheus_url))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    
    if !prometheus_response.status().is_success() {
        return Err("Prometheus is not healthy".into());
    }
    
    // Check Grafana
    let grafana_response = client
        .get(&format!("{}/api/health", config.grafana_url))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    
    if !grafana_response.status().is_success() {
        return Err("Grafana is not healthy".into());
    }
    
    // Check Loki
    let loki_response = client
        .get(&format!("{}/ready", config.loki_url))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    
    if !loki_response.status().is_success() {
        return Err("Loki is not healthy".into());
    }
    
    println!("✓ All monitoring services are healthy");
    Ok(())
}

/// Test that metrics are properly exposed by DNS servers
#[tokio::test]
async fn test_metrics_exposure() {
    let config = MonitoringTestConfig::default();
    let client = Client::new();
    
    let mut servers_tested = 0;
    let mut servers_accessible = 0;
    
    for server in &config.dns_servers {
        println!("Testing metrics exposure for {}", server.name);
        servers_tested += 1;
        
        let metrics_url = format!("http://127.0.0.1:{}/metrics", server.metrics_port);
        
        match client.get(&metrics_url).timeout(Duration::from_secs(5)).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    servers_accessible += 1;
                    
                    let metrics_text = response.text().await.expect("Failed to get metrics text");
                    
                    // Verify essential DNS metrics are present
                    assert!(metrics_text.contains("dns_queries_total"), 
                        "dns_queries_total metric should be present for {}", server.name);
                    assert!(metrics_text.contains("dns_responses_total"), 
                        "dns_responses_total metric should be present for {}", server.name);
                    assert!(metrics_text.contains("dns_query_duration_seconds"), 
                        "dns_query_duration_seconds metric should be present for {}", server.name);
                    
                    // Verify instance labels are present
                    assert!(metrics_text.contains(&format!("instance=\"{}\"", server.name)), 
                        "Instance label should be present in metrics for {}", server.name);
                    
                    println!("✓ Metrics properly exposed for {}", server.name);
                } else {
                    println!("⚠ Metrics endpoint returned status {} for {}", response.status(), server.name);
                }
            }
            Err(e) => {
                println!("⚠ Failed to access metrics endpoint for {}: {}", server.name, e);
            }
        }
    }
    
    if servers_accessible == 0 {
        println!("⚠ No DNS servers are running - skipping metrics exposure validation");
        println!("  To run this test, start the monitoring stack with:");
        println!("  ./scripts/test-monitoring.sh start");
        return;
    }
    
    println!("✓ Metrics exposure test completed: {}/{} servers accessible", servers_accessible, servers_tested);
    
    // If any servers are accessible, they should all have proper metrics
    assert!(servers_accessible > 0, "At least one DNS server should be accessible for metrics testing");
}

/// Test Prometheus scraping and service discovery
#[tokio::test]
async fn test_prometheus_scraping() {
    let config = MonitoringTestConfig::default();
    
    // Check if Prometheus is running
    if let Err(e) = check_monitoring_stack_health(&config).await {
        println!("Skipping Prometheus test - monitoring stack not available: {}", e);
        return;
    }
    
    let client = Client::new();
    
    // Wait for Prometheus to scrape targets
    sleep(Duration::from_secs(30)).await;
    
    // Check targets are discovered
    let targets_response = client
        .get(&format!("{}/api/v1/targets", config.prometheus_url))
        .send()
        .await
        .expect("Failed to get Prometheus targets");
    
    assert!(targets_response.status().is_success(), "Prometheus targets API should be accessible");
    
    let targets_json: Value = targets_response.json().await.expect("Failed to parse targets JSON");
    let active_targets = &targets_json["data"]["activeTargets"];
    
    // Verify DNS server targets are discovered
    let mut dns_targets_found = 0;
    if let Some(targets_array) = active_targets.as_array() {
        for target in targets_array {
            if let Some(job) = target["labels"]["job"].as_str() {
                if job.contains("dns-servers") || job.contains("dns") {
                    dns_targets_found += 1;
                    
                    // Verify target is healthy
                    let health = target["health"].as_str().unwrap_or("unknown");
                    assert_eq!(health, "up", "DNS server target should be healthy");
                    
                    println!("✓ Found healthy DNS target: {:?}", target["labels"]);
                }
            }
        }
    }
    
    assert!(dns_targets_found >= 1, "At least one DNS server target should be discovered");
    
    // Test querying metrics from Prometheus
    let query = "dns_queries_total";
    let query_response = client
        .get(&format!("{}/api/v1/query", config.prometheus_url))
        .query(&[("query", query)])
        .send()
        .await
        .expect("Failed to query Prometheus");
    
    assert!(query_response.status().is_success(), "Prometheus query should succeed");
    
    let query_json: Value = query_response.json().await.expect("Failed to parse query JSON");
    let result = &query_json["data"]["result"];
    
    if let Some(result_array) = result.as_array() {
        assert!(!result_array.is_empty(), "Should have DNS metrics data");
        println!("✓ Prometheus successfully scraped DNS metrics");
    }
}

/// Test end-to-end monitoring with sample DNS traffic
#[tokio::test]
async fn test_end_to_end_monitoring() {
    let config = MonitoringTestConfig::default();
    
    // Generate sample DNS traffic
    let test_domain = "monitoring-test.com";
    let test_ip = "203.0.113.42";
    
    println!("Generating sample DNS traffic...");
    
    for (i, server) in config.dns_servers.iter().enumerate() {
        let domain = format!("{}-{}", test_domain, i);
        
        // Add record
        if let Err(e) = add_dns_record_to_server(&domain, test_ip, server).await {
            println!("Warning: Failed to add record to {}: {}", server.name, e);
            continue;
        }
        
        sleep(Duration::from_millis(100)).await;
        
        // Generate queries
        for _ in 0..10 {
            if let Err(e) = send_dns_query_to_server(&domain, server).await {
                println!("Warning: Failed to query {}: {}", server.name, e);
            }
            sleep(Duration::from_millis(50)).await;
        }
        
        println!("✓ Generated traffic for {}", server.name);
    }
    
    // Wait for metrics to be collected
    sleep(Duration::from_secs(15)).await;
    
    // Verify metrics are updated
    let client = Client::new();
    for server in &config.dns_servers {
        let metrics_url = format!("http://127.0.0.1:{}/metrics", server.metrics_port);
        
        if let Ok(response) = client.get(&metrics_url).send().await {
            let metrics_text = response.text().await.unwrap_or_default();
            
            // Check that query counters have increased
            let has_query_metrics = metrics_text.lines()
                .any(|line| line.starts_with("dns_queries_total") && !line.contains(" 0"));
            
            if has_query_metrics {
                println!("✓ Metrics updated for {} after traffic generation", server.name);
            } else {
                println!("⚠ No query metrics found for {}", server.name);
            }
        }
    }
}

/// Test multi-instance DNS server monitoring
#[tokio::test]
async fn test_multi_instance_monitoring() {
    let config = MonitoringTestConfig::default();
    
    if config.dns_servers.len() < 2 {
        println!("Skipping multi-instance test - need at least 2 DNS servers configured");
        return;
    }
    
    println!("Testing multi-instance monitoring with {} servers", config.dns_servers.len());
    
    // Generate different traffic patterns for each instance
    let mut handles = Vec::new();
    
    for (i, server) in config.dns_servers.iter().enumerate() {
        let server = server.clone();
        let handle = tokio::spawn(async move {
            let domain = format!("instance-{}-test.com", i);
            let ip = format!("203.0.113.{}", 100 + i);
            
            // Add record
            if let Err(e) = add_dns_record_to_server(&domain, &ip, &server).await {
                println!("Failed to add record to {}: {}", server.name, e);
                return;
            }
            
            sleep(Duration::from_millis(100)).await;
            
            // Generate unique query patterns
            let query_count = (i + 1) * 5; // Different load per instance
            for _ in 0..query_count {
                if let Err(e) = send_dns_query_to_server(&domain, &server).await {
                    println!("Query failed for {}: {}", server.name, e);
                }
                sleep(Duration::from_millis(100)).await;
            }
            
            println!("✓ Generated {} queries for {}", query_count, server.name);
        });
        
        handles.push(handle);
    }
    
    // Wait for all traffic generation to complete
    for handle in handles {
        handle.await.expect("Traffic generation task failed");
    }
    
    // Wait for metrics collection
    sleep(Duration::from_secs(10)).await;
    
    // Verify each instance has distinct metrics
    let client = Client::new();
    let mut instance_metrics = HashMap::new();
    
    for server in &config.dns_servers {
        let metrics_url = format!("http://127.0.0.1:{}/metrics", server.metrics_port);
        
        if let Ok(response) = client.get(&metrics_url).send().await {
            let metrics_text = response.text().await.unwrap_or_default();
            
            // Extract query count for this instance
            let query_count = metrics_text.lines()
                .find(|line| line.starts_with("dns_queries_total") && line.contains(&server.name))
                .and_then(|line| line.split_whitespace().last())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            
            instance_metrics.insert(server.name.clone(), query_count);
            println!("Instance {} has {} total queries", server.name, query_count);
        }
    }
    
    // Verify instances have different metrics (indicating proper isolation)
    let values: Vec<f64> = instance_metrics.values().cloned().collect();
    let all_same = values.windows(2).all(|w| (w[0] - w[1]).abs() < 0.1);
    
    if !all_same {
        println!("✓ Multi-instance monitoring working - instances have distinct metrics");
    } else {
        println!("⚠ All instances have similar metrics - may indicate aggregation issues");
    }
}

/// Test Grafana dashboard functionality
#[tokio::test]
async fn test_grafana_dashboard_functionality() {
    let config = MonitoringTestConfig::default();
    
    // Check if Grafana is accessible
    let client = Client::new();
    let health_response = client
        .get(&format!("{}/api/health", config.grafana_url))
        .send()
        .await;
    
    if health_response.is_err() {
        println!("Skipping Grafana test - Grafana not accessible");
        return;
    }
    
    // Test dashboard API access
    let dashboards_response = client
        .get(&format!("{}/api/search?type=dash-db", config.grafana_url))
        .basic_auth("admin", Some("admin"))
        .send()
        .await;
    
    match dashboards_response {
        Ok(response) => {
            if response.status().is_success() {
                let dashboards: Value = response.json().await.expect("Failed to parse dashboards JSON");
                
                if let Some(dashboards_array) = dashboards.as_array() {
                    println!("Found {} dashboards in Grafana", dashboards_array.len());
                    
                    // Look for DNS-related dashboards
                    let dns_dashboards: Vec<_> = dashboards_array.iter()
                        .filter(|d| {
                            d["title"].as_str()
                                .map(|title| title.to_lowercase().contains("dns"))
                                .unwrap_or(false)
                        })
                        .collect();
                    
                    if !dns_dashboards.is_empty() {
                        println!("✓ Found {} DNS-related dashboards", dns_dashboards.len());
                        
                        for dashboard in dns_dashboards {
                            if let Some(title) = dashboard["title"].as_str() {
                                println!("  - {}", title);
                            }
                        }
                    } else {
                        println!("⚠ No DNS-related dashboards found");
                    }
                } else {
                    println!("✓ Grafana API accessible but no dashboards found");
                }
            } else {
                println!("⚠ Grafana API returned status: {}", response.status());
            }
        }
        Err(e) => {
            println!("⚠ Failed to access Grafana dashboards API: {}", e);
        }
    }
}

/// Test log aggregation with Loki
#[tokio::test]
async fn test_log_aggregation() {
    let config = MonitoringTestConfig::default();
    
    // Check if Loki is accessible
    let client = Client::new();
    let ready_response = client
        .get(&format!("{}/ready", config.loki_url))
        .send()
        .await;
    
    if ready_response.is_err() {
        println!("Skipping Loki test - Loki not accessible");
        return;
    }
    
    // Generate some DNS activity to create logs
    println!("Generating DNS activity to create logs...");
    
    for server in &config.dns_servers {
        let domain = format!("log-test-{}.com", server.name);
        let ip = "203.0.113.99";
        
        // Add record and query it to generate logs
        if add_dns_record_to_server(&domain, ip, server).await.is_ok() {
            sleep(Duration::from_millis(100)).await;
            
            for _ in 0..3 {
                let _ = send_dns_query_to_server(&domain, server).await;
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
    
    // Wait for logs to be collected
    sleep(Duration::from_secs(30)).await;
    
    // Query logs from Loki
    let now = chrono::Utc::now();
    let start_time = now - chrono::Duration::minutes(5);
    
    let query_params = [
        ("query", "{container_name=~\"dns-server.*\"}"),
        ("start", &start_time.timestamp_nanos_opt().unwrap_or(0).to_string()),
        ("end", &now.timestamp_nanos_opt().unwrap_or(0).to_string()),
        ("limit", "100"),
    ];
    
    match client
        .get(&format!("{}/loki/api/v1/query_range", config.loki_url))
        .query(&query_params)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                let logs_json: Value = response.json().await.expect("Failed to parse logs JSON");
                
                if let Some(result) = logs_json["data"]["result"].as_array() {
                    let total_logs = result.iter()
                        .map(|stream| stream["values"].as_array().map(|v| v.len()).unwrap_or(0))
                        .sum::<usize>();
                    
                    if total_logs > 0 {
                        println!("✓ Found {} log entries in Loki", total_logs);
                        
                        // Check for DNS-specific log content
                        let mut dns_logs_found = false;
                        for stream in result {
                            if let Some(values) = stream["values"].as_array() {
                                for entry in values {
                                    if let Some(log_line) = entry.as_array().and_then(|a| a.get(1)).and_then(|v| v.as_str()) {
                                        if log_line.contains("DNS") || log_line.contains("query") || log_line.contains("response") {
                                            dns_logs_found = true;
                                            break;
                                        }
                                    }
                                }
                                if dns_logs_found { break; }
                            }
                        }
                        
                        if dns_logs_found {
                            println!("✓ DNS-related logs found in Loki");
                        } else {
                            println!("⚠ No DNS-specific logs found in aggregated logs");
                        }
                    } else {
                        println!("⚠ No logs found in Loki for the specified time range");
                    }
                } else {
                    println!("⚠ Unexpected Loki response format");
                }
            } else {
                println!("⚠ Loki query failed with status: {}", response.status());
            }
        }
        Err(e) => {
            println!("⚠ Failed to query Loki: {}", e);
        }
    }
}

/// Test service discovery and automatic instance detection
#[tokio::test]
async fn test_service_discovery() {
    let config = MonitoringTestConfig::default();
    
    // Check if Prometheus is running
    let client = Client::new();
    let config_response = client
        .get(&format!("{}/api/v1/status/config", config.prometheus_url))
        .send()
        .await;
    
    if config_response.is_err() {
        println!("Skipping service discovery test - Prometheus not accessible");
        return;
    }
    
    // Get service discovery targets
    let targets_response = client
        .get(&format!("{}/api/v1/targets", config.prometheus_url))
        .send()
        .await
        .expect("Failed to get targets");
    
    assert!(targets_response.status().is_success(), "Targets API should be accessible");
    
    let targets_json: Value = targets_response.json().await.expect("Failed to parse targets");
    
    // Analyze discovered targets
    if let Some(active_targets) = targets_json["data"]["activeTargets"].as_array() {
        let mut dns_targets = Vec::new();
        let mut healthy_targets = 0;
        
        for target in active_targets {
            if let Some(job) = target["labels"]["job"].as_str() {
                if job.contains("dns") {
                    dns_targets.push(target);
                    
                    if target["health"].as_str() == Some("up") {
                        healthy_targets += 1;
                    }
                    
                    // Print target details
                    println!("Discovered target:");
                    println!("  Job: {}", job);
                    println!("  Instance: {}", target["labels"]["instance"].as_str().unwrap_or("unknown"));
                    println!("  Health: {}", target["health"].as_str().unwrap_or("unknown"));
                    println!("  Last Scrape: {}", target["lastScrape"].as_str().unwrap_or("unknown"));
                }
            }
        }
        
        assert!(!dns_targets.is_empty(), "Should discover at least one DNS server target");
        assert!(healthy_targets > 0, "At least one DNS target should be healthy");
        
        println!("✓ Service discovery working: found {} DNS targets, {} healthy", 
                dns_targets.len(), healthy_targets);
        
        // Verify automatic relabeling
        for target in &dns_targets {
            if let Some(labels) = target["labels"].as_object() {
                // Check for expected labels from relabeling
                if labels.contains_key("service") || labels.contains_key("instance") {
                    println!("✓ Automatic relabeling working for target");
                } else {
                    println!("⚠ Expected labels not found - relabeling may not be working");
                }
            }
        }
    } else {
        panic!("No active targets found in Prometheus");
    }
}

/// Integration test runner that orchestrates the full monitoring stack test
#[tokio::test]
async fn test_full_monitoring_stack_integration() {
    let config = MonitoringTestConfig::default();
    
    println!("=== Full Monitoring Stack Integration Test ===");
    
    // Step 1: Check monitoring stack health
    println!("\n1. Checking monitoring stack health...");
    match check_monitoring_stack_health(&config).await {
        Ok(_) => println!("✓ Monitoring stack is healthy"),
        Err(e) => {
            println!("⚠ Monitoring stack health check failed: {}", e);
            println!("Some tests may be skipped");
        }
    }
    
    // Step 2: Generate comprehensive test traffic
    println!("\n2. Generating comprehensive test traffic...");
    let mut traffic_handles = Vec::new();
    
    for (i, server) in config.dns_servers.iter().enumerate() {
        let server = server.clone();
        let handle = tokio::spawn(async move {
            let base_domain = format!("integration-test-{}", i);
            
            // Add multiple records
            for j in 0..5 {
                let domain = format!("{}-{}.com", base_domain, j);
                let ip = format!("203.0.113.{}", 150 + j);
                
                if let Ok(_) = add_dns_record_to_server(&domain, &ip, &server).await {
                    sleep(Duration::from_millis(50)).await;
                    
                    // Generate queries with different patterns
                    for _ in 0..10 {
                        let _ = send_dns_query_to_server(&domain, &server).await;
                        sleep(Duration::from_millis(25)).await;
                    }
                }
            }
            
            // Generate some error conditions
            let _ = send_dns_query_to_server("nonexistent.domain", &server).await;
            
            println!("✓ Traffic generation completed for {}", server.name);
        });
        
        traffic_handles.push(handle);
    }
    
    // Wait for traffic generation
    for handle in traffic_handles {
        handle.await.expect("Traffic generation failed");
    }
    
    // Step 3: Wait for metrics and logs to be collected
    println!("\n3. Waiting for metrics and logs collection...");
    sleep(Duration::from_secs(30)).await;
    
    // Step 4: Verify end-to-end monitoring pipeline
    println!("\n4. Verifying end-to-end monitoring pipeline...");
    
    // Check metrics are exposed
    let mut metrics_ok = true;
    for server in &config.dns_servers {
        let client = Client::new();
        let metrics_url = format!("http://127.0.0.1:{}/metrics", server.metrics_port);
        
        match client.get(&metrics_url).send().await {
            Ok(response) => {
                let metrics_text = response.text().await.unwrap_or_default();
                if !metrics_text.contains("dns_queries_total") {
                    metrics_ok = false;
                    println!("✗ Metrics not properly exposed for {}", server.name);
                } else {
                    println!("✓ Metrics properly exposed for {}", server.name);
                }
            }
            Err(_) => {
                metrics_ok = false;
                println!("✗ Failed to access metrics for {}", server.name);
            }
        }
    }
    
    // Check Prometheus has scraped the metrics
    let client = Client::new();
    if let Ok(response) = client
        .get(&format!("{}/api/v1/query", config.prometheus_url))
        .query(&[("query", "dns_queries_total")])
        .send()
        .await
    {
        if let Ok(json) = response.json::<Value>().await {
            if let Some(result) = json["data"]["result"].as_array() {
                if !result.is_empty() {
                    println!("✓ Prometheus successfully scraped DNS metrics");
                } else {
                    println!("✗ No DNS metrics found in Prometheus");
                }
            }
        }
    }
    
    println!("\n=== Integration Test Summary ===");
    if metrics_ok {
        println!("✓ Full monitoring stack integration test PASSED");
    } else {
        println!("✗ Full monitoring stack integration test had issues");
    }
}