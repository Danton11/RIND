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
/// Returns `None` for types we don't currently serve (e.g. SOA, SRV, CAA).
fn qtype_to_name(qtype: u16) -> Option<&'static str> {
    match qtype {
        1 => Some("A"),
        2 => Some("NS"),
        5 => Some("CNAME"),
        12 => Some("PTR"),
        15 => Some("MX"),
        16 => Some("TXT"),
        28 => Some("AAAA"),
        _ => None,
    }
}

/// Collect every record matching both name and record type. Returns an empty
/// vec if nothing matches. For multi-value types (NS) this is the whole
/// RRSet; for singleton types (A/AAAA/CNAME/PTR) it's one record.
///
/// TODO: when the LMDB read path lands, iteration order becomes deterministic
/// (sorted by secondary index key). RFC 1035 §6.3.3 wants RRSet order
/// randomized for load distribution. Either shuffle here or document the
/// client-side randomization assumption.
fn find_records<'a>(
    records: &'a HashMap<String, DnsRecord>,
    name: &str,
    type_name: &str,
) -> Vec<&'a DnsRecord> {
    records
        .values()
        .filter(|r| r.name == name && r.data.type_name() == type_name)
        .collect()
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
        let response = build_response(query, &[], 1);
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
            let response = build_response(query, &[], rcode);
            return (response, rcode);
        }
    };

    // Answer encoding needs to outlive the lock drop — clone the data out.
    // RFC 2181 §5.2 requires uniform TTL across the RRSet; we clamp to the
    // min across all matches so no record is advertised longer than its own
    // stored TTL.
    let matched: Vec<&DnsRecord> = find_records(&records, &question_name, type_name);
    let owned: Vec<RecordData> = matched.iter().map(|r| r.data.clone()).collect();
    let rrset_ttl: Option<u32> = matched.iter().map(|r| r.ttl).min();
    let name_matched = name_exists(&records, &question_name);
    drop(records);

    if owned.is_empty() {
        let rcode = if name_matched {
            debug!("NODATA for {} {}", type_name, question_name);
            0
        } else {
            info!("NXDOMAIN for {}", question_name);
            3
        };
        let response = build_response(query, &[], rcode);
        return (response, rcode);
    }

    let ttl = rrset_ttl.unwrap_or(60);
    let answers: Vec<(&RecordData, u32)> = owned.iter().map(|d| (d, ttl)).collect();
    debug!(
        "Found {} {} record(s) for {}",
        answers.len(),
        type_name,
        question_name
    );
    let response = build_response(query, &answers, 0);
    (response, 0)
}
