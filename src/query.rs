use crate::packet::{build_response, DnsQuery};
use crate::update::{DnsRecord, DnsRecords, RecordData};
use log::{debug, info};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handles DNS query and returns response packet
#[allow(dead_code)]
pub async fn handle_query(query: DnsQuery, records: Arc<RwLock<DnsRecords>>) -> Vec<u8> {
    let (response, _) = handle_query_with_code(query, records).await;
    response
}

/// Map a wire qtype code to the string name of a `RecordData` variant.
/// Returns `None` for types we don't currently serve (e.g. CNAME, MX).
fn qtype_to_name(qtype: u16) -> Option<&'static str> {
    match qtype {
        1 => Some("A"),
        28 => Some("AAAA"),
        _ => None,
    }
}

/// Find the first record matching both name and record type.
fn find_record<'a>(
    records: &'a HashMap<String, DnsRecord>,
    name: &str,
    type_name: &str,
) -> Option<&'a DnsRecord> {
    records
        .values()
        .find(|r| r.name == name && r.data.type_name() == type_name)
}

/// True if any record has this name, regardless of type. Used to distinguish
/// NXDOMAIN (name doesn't exist) from NODATA (name exists, no matching type).
fn name_exists(records: &HashMap<String, DnsRecord>, name: &str) -> bool {
    records.values().any(|r| r.name == name)
}

/// Handles DNS query and returns (response packet, rcode).
pub async fn handle_query_with_code(
    query: DnsQuery,
    records: Arc<RwLock<DnsRecords>>,
) -> (Vec<u8>, u8) {
    debug!("Handling query {:?}", query);

    if query.questions.is_empty() {
        debug!("Query has no questions, returning FORMERR");
        let response = build_response(query, None, 1, 60);
        return (response, 1); // FORMERR
    }

    let records = records.read().await;

    let question = &query.questions[0];
    let question_name = question.name.clone();
    let qtype = question.qtype;

    // Unsupported / unknown qtype: if the name exists, return NODATA (NOERROR,
    // empty answer). Otherwise NXDOMAIN. Either way we have nothing to encode.
    let type_name: &str = match qtype_to_name(qtype) {
        Some(t) => t,
        None => {
            let rcode = if name_exists(&records, &question_name) {
                0 // NODATA
            } else {
                3 // NXDOMAIN
            };
            info!(
                "No handler for qtype {} ({}); rcode={}",
                qtype, question_name, rcode
            );
            let response = build_response(query, None, rcode, 60);
            return (response, rcode);
        }
    };

    // Answer encoding needs to outlive the lock drop — clone the data out.
    let found: Option<(RecordData, u32)> =
        find_record(&records, &question_name, type_name).map(|r| (r.data.clone(), r.ttl));

    match found {
        Some((data, ttl)) => {
            debug!("Found {} record for {}", type_name, question_name);
            let response = build_response(query, Some(&data), 0, ttl);
            (response, 0)
        }
        None => {
            let rcode = if name_exists(&records, &question_name) {
                // Name exists but not this type → NODATA (NOERROR + empty answer).
                debug!("NODATA for {} {}", type_name, question_name);
                0
            } else {
                info!("NXDOMAIN for {}", question_name);
                3
            };
            let response = build_response(query, None, rcode, 60);
            (response, rcode)
        }
    }
}
