use std::net::Ipv4Addr;
use crate::packet::{DnsQuery, build_response};
use log::{debug, info};
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;

/// Handles DNS query and returns response packet
pub async fn handle_query(query: DnsQuery, records: Arc<RwLock<HashMap<String, crate::update::DnsRecord>>>) -> Vec<u8> {
    let (response, _) = handle_query_with_code(query, records).await;
    response
}

/// Handles DNS query and returns response packet with response code
pub async fn handle_query_with_code(query: DnsQuery, records: Arc<RwLock<HashMap<String, crate::update::DnsRecord>>>) -> (Vec<u8>, u8) {
    debug!("Handling query {:?}", query);

    // Check if we have any questions
    if query.questions.is_empty() {
        debug!("Query has no questions, returning FORMERR");
        let response = build_response(query, Ipv4Addr::new(0, 0, 0, 0), 1, 60, "A".to_string(), "IN".to_string());
        return (response, 1); // FORMERR
    }

    let records = records.read().await;

    let question_name = &query.questions[0].name;

    let (ip, response_code, ttl, record_type, class) = if let Some(record) = records.get(question_name) {
        debug!("Found record for {}", question_name);
        (
            record.ip.unwrap_or_else(|| Ipv4Addr::new(0, 0, 0, 0)),
            0,
            record.ttl,
            record.record_type.clone(),
            record.class.clone()
        )
    } else {
        info!("No record found for {}", question_name);
        (Ipv4Addr::new(0, 0, 0, 0), 3, 60, "A".to_string(), "IN".to_string()) // NXDOMAIN
    };

    let response = build_response(query, ip, response_code, ttl, record_type, class);
    (response, response_code)
}

