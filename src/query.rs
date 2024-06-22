use std::net::Ipv4Addr;
use crate::packet::{DnsQuery, build_response};
use log::{debug, info};
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;

/// Handles a DNS query and generates a response packet.
///
/// # Parameters
/// - `query`: A `DnsQuery` representing the DNS query to handle.
/// - `records`: An `Arc<RwLock<HashMap<String, DnsRecord>>>` containing the DNS records.
///
/// # Returns
/// - `Vec<u8>`: A vector of bytes representing the DNS response packet.
pub async fn handle_query(query: DnsQuery, records: Arc<RwLock<HashMap<String, crate::update::DnsRecord>>>) -> Vec<u8> {
    debug!("Handling query {:?}", query);

    let records = records.read().await;

    let mut ip: Option<Ipv4Addr> = None;
    let mut response_code = 0;
    let mut ttl = 60;
    let mut record_type = "A".to_string();
    let mut class = "IN".to_string();

    if let Some(record) = records.get(&query.questions[0].name) {
        debug!("Found record for {}: {:?}", query.questions[0].name, record);
        ip = record.ip;
        ttl = record.ttl;
        record_type = record.record_type.clone();
        class = record.class.clone();
    } else {
        info!("No record found for {}", query.questions[0].name);
        response_code = 3; // NXDOMAIN
        ip = Some(Ipv4Addr::new(0, 0, 0, 0));
    }

    // Unwrap ip, use 0.0.0.0 if None
    let ip = ip.unwrap_or_else(|| Ipv4Addr::new(0, 0, 0, 0));

    build_response(query, ip, response_code, ttl, record_type, class)
}

