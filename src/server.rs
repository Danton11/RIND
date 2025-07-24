use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, debug, error, warn, trace, span, Level, Instrument};

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
    let server_span = span!(Level::INFO, "dns_server", bind_addr = %addr);
    let _enter = server_span.enter();

    let socket = Arc::new(UdpSocket::bind(addr).await?);
    let (tx, mut rx) = mpsc::channel::<(Vec<u8>, std::net::SocketAddr)>(1024);

    let instance_id = std::env::var("SERVER_ID").unwrap_or_else(|_| {
        format!("dns-server-{}", std::process::id())
    });

    info!(
        instance_id = %instance_id,
        bind_addr = %addr,
        channel_capacity = 1024,
        "DNS server started successfully"
    );

    // Task to receive packets and send them to the channel
    let socket_clone = Arc::clone(&socket);
    let instance_id_clone = instance_id.clone();
    tokio::spawn(async move {
        let packet_receiver_span = span!(Level::DEBUG, "packet_receiver", instance_id = %instance_id_clone);
        let mut buf = vec![0u8; 512];
        let mut packet_count = 0u64;
        
        async move {
            loop {
                match socket_clone.recv_from(&mut buf).await {
                    Ok((len, addr)) => {
                        packet_count += 1;
                        trace!(
                            client_addr = %addr,
                            packet_size = len,
                            packet_count = packet_count,
                            instance_id = %instance_id_clone,
                            "Received UDP packet"
                        );
                        
                        if tx.send((buf[..len].to_vec(), addr)).await.is_err() {
                            error!(
                                instance_id = %instance_id_clone,
                                "Packet processing channel receiver dropped, shutting down packet receiver"
                            );
                            break;
                        }
                    },
                    Err(e) => {
                        error!(
                            error = %e,
                            instance_id = %instance_id_clone,
                            "Failed to receive UDP packet"
                        );
                    }
                }
            }
        }.instrument(packet_receiver_span).await
    });

    // Handle packets from the channel
    let mut active_handlers = 0u64;
    while let Some((packet, addr)) = rx.recv().await {
        let socket_clone = Arc::clone(&socket);
        let records_clone = Arc::clone(&records);
        let metrics_clone = Arc::clone(&metrics_registry);
        let instance_id_clone = instance_id.clone();

        active_handlers += 1;
        let handler_id = active_handlers;

        tokio::spawn(async move {
            let handler_span = span!(
                Level::DEBUG, 
                "packet_handler",
                client_addr = %addr,
                handler_id = handler_id,
                instance_id = %instance_id_clone
            );
            
            handle_packet(packet, addr, socket_clone, records_clone, metrics_clone, instance_id_clone)
                .instrument(handler_span)
                .await;
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
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
    instance_id: String
) {
    let start_time = Instant::now();
    let packet_size = packet.len();

    trace!(
        client_addr = %addr,
        packet_size = packet_size,
        instance_id = %instance_id,
        "Starting DNS packet processing"
    );

    // Parse the DNS packet
    let parsing_span = span!(Level::TRACE, "packet_parsing", client_addr = %addr, instance_id = %instance_id);
    let parse_result = parsing_span.in_scope(|| {
        packet::parse(&packet)
    });

    match parse_result {
        Ok(query) => {
            let query_id = query.id;
            let question_count = query.questions.len();
            
            // Extract query details for logging and metrics
            let (query_type, query_name, _query_type_num) = if !query.questions.is_empty() {
                let question = &query.questions[0];
                let type_str = match question.qtype {
                    1 => "A",
                    28 => "AAAA", 
                    15 => "MX",
                    2 => "NS",
                    5 => "CNAME",
                    16 => "TXT",
                    12 => "PTR",
                    6 => "SOA",
                    _ => "OTHER",
                };
                (type_str, question.name.clone(), question.qtype)
            } else {
                ("UNKNOWN", "".to_string(), 0)
            };

            debug!(
                client_addr = %addr,
                query_id = query_id,
                query_type = query_type,
                query_name = %query_name,
                question_count = question_count,
                instance_id = %instance_id,
                "Successfully parsed DNS query"
            );

            // Increment query counter for metrics
            {
                let metrics = metrics_registry.read().await;
                metrics.dns_metrics().queries_total
                    .with_label_values(&[query_type, &instance_id])
                    .inc();
            }

            // Process the query
            let query_processing_span = span!(
                Level::DEBUG, 
                "query_processing",
                client_addr = %addr,
                query_id = query_id,
                query_type = query_type,
                query_name = %query_name,
                instance_id = %instance_id
            );

            let (response, response_code) = query_processing_span
                .in_scope(|| async {
                    query::handle_query_with_code(query, records).await
                })
                .await;
            
            // Calculate processing time
            let processing_duration = start_time.elapsed();
            let processing_time_ms = processing_duration.as_millis() as f64;

            // Record query processing time in metrics
            {
                let metrics = metrics_registry.read().await;
                metrics.dns_metrics().query_duration
                    .with_label_values(&[query_type, &instance_id])
                    .observe(processing_duration.as_secs_f64());
            }

            // Map response code to string for logging and metrics
            let response_code_str = match response_code {
                0 => "NOERROR",
                1 => "FORMERR", 
                2 => "SERVFAIL",
                3 => "NXDOMAIN",
                4 => "NOTIMP",
                5 => "REFUSED",
                _ => "OTHER",
            };

            // Track response codes in metrics
            {
                let metrics = metrics_registry.read().await;
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

            // Log query processing completion with performance metrics
            match response_code {
                0 => info!(
                    client_addr = %addr,
                    query_id = query_id,
                    query_type = query_type,
                    query_name = %query_name,
                    response_code = response_code,
                    response_code_str = response_code_str,
                    processing_time_ms = processing_time_ms,
                    response_size = response.len(),
                    instance_id = %instance_id,
                    "DNS query processed successfully"
                ),
                3 => debug!(
                    client_addr = %addr,
                    query_id = query_id,
                    query_type = query_type,
                    query_name = %query_name,
                    response_code = response_code,
                    response_code_str = response_code_str,
                    processing_time_ms = processing_time_ms,
                    response_size = response.len(),
                    instance_id = %instance_id,
                    "DNS query returned NXDOMAIN (domain not found)"
                ),
                _ => warn!(
                    client_addr = %addr,
                    query_id = query_id,
                    query_type = query_type,
                    query_name = %query_name,
                    response_code = response_code,
                    response_code_str = response_code_str,
                    processing_time_ms = processing_time_ms,
                    response_size = response.len(),
                    instance_id = %instance_id,
                    "DNS query completed with non-standard response code"
                )
            }

            // Send response back to client
            let response_span = span!(
                Level::TRACE, 
                "response_transmission",
                client_addr = %addr,
                query_id = query_id,
                response_size = response.len(),
                instance_id = %instance_id
            );

            if let Err(e) = response_span.in_scope(|| async {
                socket.send_to(&response, &addr).await
            }).await {
                error!(
                    client_addr = %addr,
                    query_id = query_id,
                    query_type = query_type,
                    query_name = %query_name,
                    error = %e,
                    processing_time_ms = processing_time_ms,
                    instance_id = %instance_id,
                    "Failed to send DNS response to client"
                );
                
                // Increment network error counter
                {
                    let metrics = metrics_registry.read().await;
                    metrics.dns_metrics().packet_errors_total.inc();
                }
            } else {
                trace!(
                    client_addr = %addr,
                    query_id = query_id,
                    response_size = response.len(),
                    total_time_ms = start_time.elapsed().as_millis() as f64,
                    instance_id = %instance_id,
                    "DNS response sent successfully"
                );
            }
        }
        Err(e) => {
            let total_time_ms = start_time.elapsed().as_millis() as f64;
            
            error!(
                client_addr = %addr,
                packet_size = packet_size,
                error = %e,
                processing_time_ms = total_time_ms,
                instance_id = %instance_id,
                packet_hex = %hex::encode(&packet[..packet_size.min(32)]), // Log first 32 bytes for debugging
                "Failed to parse DNS packet"
            );
            
            // Increment packet parsing error counter
            {
                let metrics = metrics_registry.read().await;
                metrics.dns_metrics().packet_errors_total.inc();
            }
        }
    }

    let total_duration = start_time.elapsed();
    trace!(
        client_addr = %addr,
        total_time_ms = total_duration.as_millis() as f64,
        instance_id = %instance_id,
        "DNS packet processing completed"
    );
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
        let instance_id = "test-server-1".to_string();
        
        // Call handle_packet
        handle_packet(packet, addr, socket, records, metrics_registry.clone(), instance_id).await;
        
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
        let instance_id = "test-server-1".to_string();
        
        // Call handle_packet
        handle_packet(packet, addr, socket, records, metrics_registry.clone(), instance_id).await;
        
        // Verify error metrics were recorded
        let metrics = metrics_registry.read().await;
        let metrics_text = metrics.gather_metrics().unwrap();
        
        // Check that packet error counter was incremented
        assert!(metrics_text.contains("dns_packet_errors_total"));
    }
}