use crate::packet::{build_response, DnsQuery};
use crate::storage::LmdbStore;
use crate::update::{DnsRecord, RecordData};
use log::{debug, info};
use std::sync::Arc;

/// Handles DNS query and returns response packet
#[allow(dead_code)]
pub async fn handle_query(query: DnsQuery, store: Arc<LmdbStore>) -> Vec<u8> {
    let (response, _) = handle_query_with_code(query, store).await;
    response
}

/// Map a wire qtype code to the string name of a `RecordData` variant.
/// Returns `None` for types we don't currently serve (e.g. CAA).
fn qtype_to_name(qtype: u16) -> Option<&'static str> {
    match qtype {
        1 => Some("A"),
        2 => Some("NS"),
        5 => Some("CNAME"),
        6 => Some("SOA"),
        12 => Some("PTR"),
        15 => Some("MX"),
        16 => Some("TXT"),
        28 => Some("AAAA"),
        33 => Some("SRV"),
        _ => None,
    }
}

/// Handles DNS query and returns (response packet, rcode).
///
/// Reads go through the `records_by_name` secondary index: one btree prefix
/// scan on the question name returns every row sharing that name, then an
/// in-memory filter peels off the matching type. This serves the NXDOMAIN
/// vs NODATA distinction from the same iterator — `all_by_name.is_empty()`
/// is NXDOMAIN, `matched.is_empty() && !all_by_name.is_empty()` is NODATA.
///
/// RRSet ordering is deterministic (secondary-index key order), not
/// randomized per RFC 1035 §6.3.3. That's a policy choice for a later task;
/// resolvers are expected to round-robin on their own anyway.
pub async fn handle_query_with_code(query: DnsQuery, store: Arc<LmdbStore>) -> (Vec<u8>, u8) {
    debug!("Handling query {:?}", query);

    if query.questions.is_empty() {
        debug!("Query has no questions, returning FORMERR");
        let response = build_response(query, &[], 1);
        return (response, 1); // FORMERR
    }

    let question = &query.questions[0];
    let question_name = question.name.clone();
    let qtype = question.qtype;

    let all_by_name = match store.find_records_by_name(&question_name) {
        Ok(v) => v,
        Err(e) => {
            info!("Storage error on query for {}: {}", question_name, e);
            let response = build_response(query, &[], 2); // SERVFAIL
            return (response, 2);
        }
    };
    let name_matched = !all_by_name.is_empty();

    // Unsupported / unknown qtype: if the name exists, return NODATA.
    // Otherwise NXDOMAIN. Either way we have nothing to encode.
    let type_name: &str = match qtype_to_name(qtype) {
        Some(t) => t,
        None => {
            let rcode = if name_matched { 0 } else { 3 };
            info!(
                "No handler for qtype {} ({}); rcode={}",
                qtype, question_name, rcode
            );
            let response = build_response(query, &[], rcode);
            return (response, rcode);
        }
    };

    // RFC 2181 §5.2: uniform TTL across the RRSet. Clamp to the min so no
    // record is advertised longer than its own stored TTL.
    let matched: Vec<&DnsRecord> = all_by_name
        .iter()
        .filter(|r| r.data.type_name() == type_name)
        .collect();
    let owned: Vec<RecordData> = matched.iter().map(|r| r.data.clone()).collect();
    let rrset_ttl: Option<u32> = matched.iter().map(|r| r.ttl).min();

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
