use std::net::{UdpSocket, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::thread;
use tokio::sync::RwLock;
use tokio::time::timeout;
use rind::packet::{parse, build_response, DnsQuery, Question};
use rind::update::{DnsRecord, DnsRecords, update_record};
use rind::server::run;

const DNS_SERVER_ADDR: &str = "127.0.0.1:12312";
const API_SERVER_ADDR: &str = "127.0.0.1:8080";

fn create_dns_query_packet(domain: &str) -> Vec<u8> {
    let mut packet = vec![
        0x12, 0x34, // ID
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

async fn send_dns_query(domain: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    
    let packet = create_dns_query_packet(domain);
    let server_addr: SocketAddr = DNS_SERVER_ADDR.parse()?;
    
    socket.send_to(&packet, server_addr)?;
    
    let mut buffer = vec![0u8; 512];
    let (len, _) = socket.recv_from(&mut buffer)?;
    buffer.truncate(len);
    
    Ok(buffer)
}

async fn add_dns_record_via_api(name: &str, ip: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        .post(&format!("http://{}/update", API_SERVER_ADDR))
        .json(&record)
        .send()
        .await?;
    
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("API request failed: {}", response.status()).into())
    }
}

#[tokio::test]
async fn test_end_to_end_record_addition() {
    // Test adding a record via API and immediately querying it
    let domain = "end-to-end-test.com";
    let ip = "203.0.113.42";
    
    // Add record via API
    let start = Instant::now();
    add_dns_record_via_api(domain, ip).await.expect("Failed to add record");
    let api_time = start.elapsed();
    
    // Small delay to ensure propagation
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    // Query the record
    let query_start = Instant::now();
    let response = send_dns_query(domain).await.expect("Failed to query DNS");
    let query_time = query_start.elapsed();
    
    let total_time = start.elapsed();
    
    // Verify response is valid
    assert!(response.len() > 12, "Response too short");
    
    // Check response flags indicate success
    let flags = u16::from_be_bytes([response[2], response[3]]);
    let response_code = flags & 0x000F;
    assert_eq!(response_code, 0, "DNS query should succeed");
    
    println!("End-to-end timing:");
    println!("  API time: {:?}", api_time);
    println!("  Query time: {:?}", query_time);
    println!("  Total time: {:?}", total_time);
    
    // Performance assertions
    assert!(total_time < Duration::from_millis(100), "End-to-end should be under 100ms");
}

#[tokio::test]
async fn test_concurrent_queries() {
    let domain = "concurrent-test.com";
    let ip = "203.0.113.100";
    
    // Add test record
    add_dns_record_via_api(domain, ip).await.expect("Failed to add record");
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    // Send multiple concurrent queries
    let num_queries = 50;
    let mut handles = Vec::new();
    
    let start = Instant::now();
    
    for _ in 0..num_queries {
        let domain = domain.to_string();
        let handle = tokio::spawn(async move {
            send_dns_query(&domain).await
        });
        handles.push(handle);
    }
    
    // Wait for all queries to complete
    let mut successful = 0;
    let mut failed = 0;
    
    for handle in handles {
        match handle.await {
            Ok(Ok(_response)) => successful += 1,
            _ => failed += 1,
        }
    }
    
    let total_time = start.elapsed();
    let qps = num_queries as f64 / total_time.as_secs_f64();
    
    println!("Concurrent query results:");
    println!("  Successful: {}/{}", successful, num_queries);
    println!("  Failed: {}", failed);
    println!("  Total time: {:?}", total_time);
    println!("  QPS: {:.2}", qps);
    
    assert!(successful >= num_queries * 95 / 100, "At least 95% should succeed");
    assert!(qps > 100.0, "Should handle at least 100 QPS");
}

#[tokio::test]
async fn test_malformed_packets() {
    let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind socket");
    socket.set_read_timeout(Some(Duration::from_millis(1000))).expect("Failed to set timeout");
    
    let server_addr: SocketAddr = DNS_SERVER_ADDR.parse().expect("Invalid server address");
    
    let malformed_packets = vec![
        vec![], // Empty packet
        vec![0x00; 5], // Too short
        vec![0xFF; 12], // Invalid header
        vec![0x12, 0x34, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // No questions
    ];
    
    for (i, packet) in malformed_packets.iter().enumerate() {
        println!("Testing malformed packet {}", i);
        
        // Send malformed packet
        socket.send_to(packet, server_addr).expect("Failed to send packet");
        
        // Try to receive response (should timeout for malformed packets)
        let mut buffer = vec![0u8; 512];
        match socket.recv_from(&mut buffer) {
            Ok(_) => println!("  Packet {}: Got response (unexpected)", i),
            Err(_) => println!("  Packet {}: Timeout (expected)", i),
        }
    }
}

#[tokio::test]
async fn test_record_update_performance() {
    let domain = "update-test.com";
    let initial_ip = "203.0.113.1";
    let updated_ip = "203.0.113.2";
    
    // Add initial record
    add_dns_record_via_api(domain, initial_ip).await.expect("Failed to add initial record");
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    // Verify initial record
    let response = send_dns_query(domain).await.expect("Failed to query initial record");
    assert!(response.len() > 12);
    
    // Update record and measure timing
    let start = Instant::now();
    add_dns_record_via_api(domain, updated_ip).await.expect("Failed to update record");
    
    // Query updated record
    let response = send_dns_query(domain).await.expect("Failed to query updated record");
    let update_time = start.elapsed();
    
    assert!(response.len() > 12);
    
    println!("Record update timing: {:?}", update_time);
    assert!(update_time < Duration::from_millis(50), "Record update should be under 50ms");
}

#[tokio::test]
async fn test_extreme_domain_names() {
    let test_domains = vec![
        "a.com",                                    // Short domain
        "very-long-subdomain-name-for-testing.example.com", // Long subdomain
        "test.co.uk",                              // Multiple TLD parts
        "123.numeric.com",                         // Numeric subdomain
        "test-with-hyphens.example.com",          // Hyphens
    ];
    
    for domain in test_domains {
        println!("Testing domain: {}", domain);
        
        // Add record
        let ip = "203.0.113.123";
        add_dns_record_via_api(domain, ip).await.expect(&format!("Failed to add record for {}", domain));
        tokio::time::sleep(Duration::from_millis(5)).await;
        
        // Query record
        let response = send_dns_query(domain).await.expect(&format!("Failed to query {}", domain));
        
        // Verify response
        assert!(response.len() > 12, "Response too short for {}", domain);
        let flags = u16::from_be_bytes([response[2], response[3]]);
        let response_code = flags & 0x000F;
        assert_eq!(response_code, 0, "Query for {} should succeed", domain);
    }
}

#[tokio::test]
async fn test_sustained_load() {
    let domain = "load-test.com";
    let ip = "203.0.113.200";
    
    // Add test record
    add_dns_record_via_api(domain, ip).await.expect("Failed to add record");
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    // Run sustained queries for 5 seconds
    let duration = Duration::from_secs(5);
    let start = Instant::now();
    let mut query_count = 0;
    let mut error_count = 0;
    
    while start.elapsed() < duration {
        match send_dns_query(domain).await {
            Ok(_) => query_count += 1,
            Err(_) => error_count += 1,
        }
        
        // Small delay to prevent overwhelming
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    
    let actual_duration = start.elapsed();
    let qps = query_count as f64 / actual_duration.as_secs_f64();
    let error_rate = error_count as f64 / (query_count + error_count) as f64 * 100.0;
    
    println!("Sustained load results:");
    println!("  Duration: {:?}", actual_duration);
    println!("  Queries: {}", query_count);
    println!("  Errors: {}", error_count);
    println!("  QPS: {:.2}", qps);
    println!("  Error rate: {:.2}%", error_rate);
    
    assert!(qps > 50.0, "Should maintain at least 50 QPS");
    assert!(error_rate < 5.0, "Error rate should be under 5%");
}