use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use log::{info, debug, error};

use crate::packet;
use crate::query;
use crate::update::DnsRecords;
use crate::metrics::MetricsRegistry;

/// Runs the DNS server on the specified address
pub async fn run(
    addr: &str, 
    records: Arc<RwLock<DnsRecords>>, 
    metrics_registry: Arc<RwLock<MetricsRegistry>>
) -> Result<(), Box<dyn Error>> {
    let socket = Arc::new(UdpSocket::bind(addr).await?);
    let (tx, mut rx) = mpsc::channel::<(Vec<u8>, std::net::SocketAddr)>(1024);

    info!("DNS server listening on {}", addr);

    // Task to receive packets and send them to the channel
    let socket_clone = Arc::clone(&socket);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 512];
        loop {
            match socket_clone.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    debug!("Received packet from {}: {:?}", addr, &buf[..len]);
                    if tx.send((buf[..len].to_vec(), addr)).await.is_err() {
                        error!("Receiver dropped");
                        break;
                    }
                },
                Err(e) => {
                    error!("Failed to receive packet: {}", e);
                }
            }
        }
    });

    // Handle packets from the channel
    while let Some((packet, addr)) = rx.recv().await {
        let socket_clone = Arc::clone(&socket);
        let records_clone = Arc::clone(&records);
        let metrics_clone = Arc::clone(&metrics_registry);

        tokio::spawn(async move {
            debug!("Handling packet from {}", addr);
            handle_packet(packet, addr, socket_clone, records_clone, metrics_clone).await;
        });
    }

    Ok(())
}

/// Parses DNS packet, processes query, and sends response
async fn handle_packet(
    packet: Vec<u8>, 
    addr: std::net::SocketAddr, 
    socket: Arc<UdpSocket>, 
    records: Arc<RwLock<DnsRecords>>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>
) {
    let start_time = Instant::now();
    let instance_id = std::env::var("SERVER_ID").unwrap_or_else(|_| {
        format!("dns-server-{}", std::process::id())
    });

    match packet::parse(&packet) {
        Ok(query) => {
            debug!("Parsed query from {}", addr);
            
            // Extract query type for metrics
            let query_type = if !query.questions.is_empty() {
                match query.questions[0].qtype {
                    1 => "A",
                    28 => "AAAA", 
                    15 => "MX",
                    2 => "NS",
                    5 => "CNAME",
                    16 => "TXT",
                    12 => "PTR",
                    6 => "SOA",
                    _ => "OTHER",
                }
            } else {
                "UNKNOWN"
            };

            // Increment query counter
            {
                let metrics = metrics_registry.read().await;
                metrics.dns_metrics().queries_total
                    .with_label_values(&[query_type, &instance_id])
                    .inc();
            }

            let (response, response_code) = query::handle_query_with_code(query, records).await;
            
            // Record query processing time
            let duration = start_time.elapsed();
            {
                let metrics = metrics_registry.read().await;
                metrics.dns_metrics().query_duration
                    .with_label_values(&[query_type, &instance_id])
                    .observe(duration.as_secs_f64());
            }

            // Track response codes
            {
                let metrics = metrics_registry.read().await;
                let response_code_str = match response_code {
                    0 => "NOERROR",
                    1 => "FORMERR", 
                    2 => "SERVFAIL",
                    3 => "NXDOMAIN",
                    4 => "NOTIMP",
                    5 => "REFUSED",
                    _ => "OTHER",
                };

                metrics.dns_metrics().responses_total
                    .with_label_values(&[response_code_str, &instance_id])
                    .inc();

                // Track specific error types
                match response_code {
                    3 => metrics.dns_metrics().nxdomain_total.inc(),
                    2 => metrics.dns_metrics().servfail_total.inc(),
                    _ => {}
                }
            }

            if let Err(e) = socket.send_to(&response, &addr).await {
                error!("Failed to send response to {}: {}", addr, e);
                // Increment network error counter
                {
                    let metrics = metrics_registry.read().await;
                    metrics.dns_metrics().packet_errors_total.inc();
                }
            } else {
                debug!("Sent response to {}", addr);
            }
        }
        Err(e) => {
            error!("Failed to parse packet: {}", e);
            // Increment packet parsing error counter
            {
                let metrics = metrics_registry.read().await;
                metrics.dns_metrics().packet_errors_total.inc();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricsRegistry;
    use crate::update::DnsRecord;
    use std::collections::HashMap;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn test_handle_packet_with_metrics() {
        // Create test data
        let mut records = HashMap::new();
        records.insert("test.com".to_string(), DnsRecord {
            name: "test.com".to_string(),
            ip: Some(Ipv4Addr::new(1, 2, 3, 4)),
            ttl: 300,
            record_type: "A".to_string(),
            class: "IN".to_string(),
            value: None,
        });
        let records = Arc::new(RwLock::new(records));
        
        // Create metrics registry
        let metrics_registry = Arc::new(RwLock::new(MetricsRegistry::new().unwrap()));
        
        // Create a simple DNS query packet for test.com A record
        let packet = vec![
            0x12, 0x34, // ID
            0x01, 0x00, // Flags (standard query)
            0x00, 0x01, // QDCOUNT
            0x00, 0x00, // ANCOUNT
            0x00, 0x00, // NSCOUNT
            0x00, 0x00, // ARCOUNT
            // Question: test.com A IN
            0x04, b't', b'e', b's', b't',
            0x03, b'c', b'o', b'm',
            0x00, // End of name
            0x00, 0x01, // Type A
            0x00, 0x01, // Class IN
        ];
        
        // Create a mock socket (we won't actually send anything)
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = "127.0.0.1:12345".parse().unwrap();
        
        // Call handle_packet
        handle_packet(packet, addr, socket, records, metrics_registry.clone()).await;
        
        // Verify metrics were recorded
        let metrics = metrics_registry.read().await;
        let metrics_text = metrics.gather_metrics().unwrap();
        
        // Check that query counter was incremented
        assert!(metrics_text.contains("dns_queries_total"));
        assert!(metrics_text.contains("dns_responses_total"));
        assert!(metrics_text.contains("dns_query_duration_seconds"));
    }

    #[tokio::test]
    async fn test_handle_packet_parsing_error() {
        // Create empty records
        let records = Arc::new(RwLock::new(HashMap::new()));
        
        // Create metrics registry
        let metrics_registry = Arc::new(RwLock::new(MetricsRegistry::new().unwrap()));
        
        // Create an invalid packet (too short)
        let packet = vec![0x12, 0x34]; // Only 2 bytes, should cause parsing error
        
        // Create a mock socket
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = "127.0.0.1:12345".parse().unwrap();
        
        // Call handle_packet
        handle_packet(packet, addr, socket, records, metrics_registry.clone()).await;
        
        // Verify error metrics were recorded
        let metrics = metrics_registry.read().await;
        let metrics_text = metrics.gather_metrics().unwrap();
        
        // Check that packet error counter was incremented
        assert!(metrics_text.contains("dns_packet_errors_total"));
    }
}