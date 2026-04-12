use chrono::Utc;
use rind::packet::{DnsQuery, Question};
use rind::query::handle_query_with_code;
use rind::update::{DnsRecord, DnsRecords, RecordData};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

fn make_record(name: &str, data: RecordData) -> DnsRecord {
    make_record_ttl(name, data, 300)
}

fn make_record_ttl(name: &str, data: RecordData, ttl: u32) -> DnsRecord {
    let now = Utc::now();
    DnsRecord {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        ttl,
        class: "IN".to_string(),
        data,
        created_at: now,
        updated_at: now,
    }
}

fn store(records: Vec<DnsRecord>) -> Arc<RwLock<DnsRecords>> {
    let mut map = DnsRecords::new();
    for r in records {
        map.insert(r.id.clone(), r);
    }
    Arc::new(RwLock::new(map))
}

fn query(name: &str, qtype: u16) -> DnsQuery {
    DnsQuery {
        id: 0x4242,
        flags: 0x0100,
        questions: vec![Question {
            name: name.to_string(),
            qtype,
            qclass: 1,
        }],
        has_opt: false,
        opt_payload_size: 512,
    }
}

#[tokio::test]
async fn a_record_query_returns_noerror_with_answer() {
    let records = store(vec![make_record(
        "a.example.com",
        RecordData::A {
            ip: Ipv4Addr::new(1, 2, 3, 4),
        },
    )]);
    let (response, rcode) = handle_query_with_code(query("a.example.com", 1), records).await;
    assert_eq!(rcode, 0);
    // ANCOUNT == 1
    assert_eq!(u16::from_be_bytes([response[6], response[7]]), 1);
}

#[tokio::test]
async fn aaaa_record_query_returns_noerror_with_answer() {
    let records = store(vec![make_record(
        "v6.example.com",
        RecordData::Aaaa {
            ip: Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1),
        },
    )]);
    let (response, rcode) = handle_query_with_code(query("v6.example.com", 28), records).await;
    assert_eq!(rcode, 0);
    assert_eq!(u16::from_be_bytes([response[6], response[7]]), 1);
}

#[tokio::test]
async fn name_exists_but_wrong_type_returns_nodata() {
    // Only an A record for this name; ask for AAAA → NODATA (rcode 0, ANCOUNT 0).
    let records = store(vec![make_record(
        "only-a.example.com",
        RecordData::A {
            ip: Ipv4Addr::new(1, 2, 3, 4),
        },
    )]);
    let (response, rcode) = handle_query_with_code(query("only-a.example.com", 28), records).await;
    assert_eq!(rcode, 0, "NODATA must use NOERROR rcode");
    assert_eq!(
        u16::from_be_bytes([response[6], response[7]]),
        0,
        "NODATA must have empty answer section"
    );
}

#[tokio::test]
async fn nodata_reverse_direction_aaaa_only_vs_a_query() {
    let records = store(vec![make_record(
        "only-v6.example.com",
        RecordData::Aaaa {
            ip: Ipv6Addr::LOCALHOST,
        },
    )]);
    let (response, rcode) = handle_query_with_code(query("only-v6.example.com", 1), records).await;
    assert_eq!(rcode, 0);
    assert_eq!(u16::from_be_bytes([response[6], response[7]]), 0);
}

#[tokio::test]
async fn missing_name_returns_nxdomain() {
    let records = store(vec![]);
    let (_response, rcode) = handle_query_with_code(query("ghost.example.com", 1), records).await;
    assert_eq!(rcode, 3);
}

#[tokio::test]
async fn unsupported_qtype_on_existing_name_is_nodata() {
    // SRV (33) isn't served; name exists as A → NODATA, not NXDOMAIN.
    // Exercises the `qtype_to_name` None branch specifically (vs. the
    // known-qtype-no-match path which hits a different branch).
    let records = store(vec![make_record(
        "srv.example.com",
        RecordData::A {
            ip: Ipv4Addr::new(1, 2, 3, 4),
        },
    )]);
    let (_response, rcode) = handle_query_with_code(query("srv.example.com", 33), records).await;
    assert_eq!(rcode, 0);
}

#[tokio::test]
async fn unsupported_qtype_on_missing_name_is_nxdomain() {
    let records = store(vec![]);
    let (_response, rcode) = handle_query_with_code(query("nope.example.com", 33), records).await;
    assert_eq!(rcode, 3);
}

#[tokio::test]
async fn both_a_and_aaaa_coexist_for_same_name() {
    let records = store(vec![
        make_record(
            "dual.example.com",
            RecordData::A {
                ip: Ipv4Addr::new(1, 2, 3, 4),
            },
        ),
        make_record(
            "dual.example.com",
            RecordData::Aaaa {
                ip: Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 2),
            },
        ),
    ]);
    let (_, a_rcode) = handle_query_with_code(query("dual.example.com", 1), records.clone()).await;
    let (_, aaaa_rcode) = handle_query_with_code(query("dual.example.com", 28), records).await;
    assert_eq!(a_rcode, 0);
    assert_eq!(aaaa_rcode, 0);
}

#[tokio::test]
async fn ns_delegation_set_returns_all_records() {
    // Two NS records at the same name = delegation set. A `dig example.com NS`
    // must return both. ANCOUNT == 2.
    let records = store(vec![
        make_record(
            "example.com",
            RecordData::Ns {
                target: "ns1.example.com".to_string(),
            },
        ),
        make_record(
            "example.com",
            RecordData::Ns {
                target: "ns2.example.com".to_string(),
            },
        ),
    ]);
    let (response, rcode) = handle_query_with_code(query("example.com", 2), records).await;
    assert_eq!(rcode, 0);
    assert_eq!(
        u16::from_be_bytes([response[6], response[7]]),
        2,
        "RRSet of size 2 must produce ANCOUNT=2"
    );
}

#[tokio::test]
async fn ns_rrset_ttl_is_min_clamped() {
    // RFC 2181 §5.2: all RRs in an RRSet must share a TTL. We store per-record
    // TTLs and clamp to the min at read time. Two NS records with TTLs 100 and
    // 500 should both be emitted with TTL=100.
    let records = store(vec![
        make_record_ttl(
            "example.com",
            RecordData::Ns {
                target: "ns1.example.com".to_string(),
            },
            100,
        ),
        make_record_ttl(
            "example.com",
            RecordData::Ns {
                target: "ns2.example.com".to_string(),
            },
            500,
        ),
    ]);
    let (response, rcode) = handle_query_with_code(query("example.com", 2), records).await;
    assert_eq!(rcode, 0);
    assert_eq!(u16::from_be_bytes([response[6], response[7]]), 2);

    // Walk the response to both TTL fields and assert both equal 100.
    //
    // Layout after header(12) + question(qname + qtype(2) + qclass(2)):
    //   answer[0]: ansname + type(2) + class(2) + ttl(4) + rdlen(2) + rdata
    //   answer[1]: ansname + type(2) + class(2) + ttl(4) + rdlen(2) + rdata
    //
    // ansname for "example.com" is 1+7+1+3+1 = 13 bytes (7example3com0).
    // Constant bits of an RR header: 13 + 2 + 2 + 4 + 2 = 23, then rdata.
    // NS rdata for "nsN.example.com" = 1+3+1+7+1+3+1 = 17 bytes.
    // So each answer RR is 23 + 17 = 40 bytes.
    //
    // Question echoes back "example.com" (13 bytes) + qtype(2) + qclass(2) = 17.
    // Header = 12. First answer TTL lives at offset 12 + 17 + 13 + 2 + 2 = 46.
    // Second answer TTL lives at 46 + 40 = 86.
    let ttl1 = u32::from_be_bytes([response[46], response[47], response[48], response[49]]);
    let ttl2 = u32::from_be_bytes([response[86], response[87], response[88], response[89]]);
    assert_eq!(ttl1, 100, "first answer TTL must be min-clamped");
    assert_eq!(ttl2, 100, "second answer TTL must be min-clamped");
}
