use chrono::Utc;
use rind::packet::{DnsQuery, Question};
use rind::query::handle_query_with_code;
use rind::update::{DnsRecord, DnsRecords, RecordData};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

fn make_record(name: &str, data: RecordData) -> DnsRecord {
    let now = Utc::now();
    DnsRecord {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        ttl: 300,
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
    // MX (15) isn't served; name exists as A → NODATA, not NXDOMAIN.
    let records = store(vec![make_record(
        "mx.example.com",
        RecordData::A {
            ip: Ipv4Addr::new(1, 2, 3, 4),
        },
    )]);
    let (_response, rcode) = handle_query_with_code(query("mx.example.com", 15), records).await;
    assert_eq!(rcode, 0);
}

#[tokio::test]
async fn unsupported_qtype_on_missing_name_is_nxdomain() {
    let records = store(vec![]);
    let (_response, rcode) = handle_query_with_code(query("nope.example.com", 15), records).await;
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
