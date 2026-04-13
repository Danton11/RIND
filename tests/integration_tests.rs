//! End-to-end integration tests. Each test spawns its own in-process RIND
//! instance via `TestHarness` — no fullstack docker required, no shared state
//! between tests, no `#[ignore]`.

mod common;

use std::time::{Duration, Instant};

use common::harness::{ancount, rcode, TestHarness};
use serde_json::json;

#[tokio::test]
async fn end_to_end_record_addition() {
    let h = TestHarness::spawn().await;
    let domain = "end-to-end.test.local";

    let start = Instant::now();
    h.create_a(domain, "203.0.113.42").await;
    let api_time = start.elapsed();

    let query_start = Instant::now();
    let response = h.query_a(domain).await;
    let query_time = query_start.elapsed();

    assert!(response.len() > 12, "response too short");
    assert_eq!(rcode(&response), 0, "rcode should be NOERROR");
    assert!(ancount(&response) >= 1, "should have at least one answer");

    println!("api: {:?}  query: {:?}", api_time, query_time);
}

#[tokio::test]
async fn concurrent_queries() {
    let h = TestHarness::spawn().await;
    let domain = "concurrent.test.local";
    h.create_a(domain, "203.0.113.100").await;

    let num_queries = 50;
    let start = Instant::now();
    let mut handles = Vec::with_capacity(num_queries);
    for _ in 0..num_queries {
        // Each query binds its own ephemeral client socket via the harness,
        // so concurrency is real — no shared socket contention.
        let dns_addr = h.dns_addr;
        let domain = domain.to_string();
        handles.push(tokio::spawn(async move {
            let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            sock.connect(dns_addr).await.unwrap();
            let packet = common::harness::build_a_query(&domain);
            sock.send(&packet).await.unwrap();
            let mut buf = vec![0u8; 512];
            let len = tokio::time::timeout(Duration::from_secs(2), sock.recv(&mut buf))
                .await
                .unwrap()
                .unwrap();
            buf.truncate(len);
            rcode(&buf)
        }));
    }

    let mut ok = 0;
    for h in handles {
        if h.await.unwrap() == 0 {
            ok += 1;
        }
    }
    let elapsed = start.elapsed();
    let qps = num_queries as f64 / elapsed.as_secs_f64();
    println!("{} ok in {:?} ({:.0} qps)", ok, elapsed, qps);

    assert!(ok >= num_queries * 95 / 100, "at least 95% must succeed");
}

#[tokio::test]
async fn malformed_packets_do_not_crash_server() {
    let h = TestHarness::spawn().await;

    // Sanity: server still answers good queries after eating junk.
    let domain = "sanity.test.local";
    h.create_a(domain, "203.0.113.7").await;

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.connect(h.dns_addr).await.unwrap();

    for bad in [
        vec![],
        vec![0u8; 5],
        vec![0xFFu8; 12],
        vec![
            0x12, 0x34, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
    ] {
        // Malformed packets may or may not draw a response; we only care
        // that the server doesn't wedge. Send and move on.
        let _ = client.send(&bad).await;
    }

    // Server still alive?
    let response = h.query_a(domain).await;
    assert_eq!(rcode(&response), 0);
}

#[tokio::test]
async fn extreme_domain_names() {
    let h = TestHarness::spawn().await;

    for domain in [
        "a.test.local",
        "very-long-subdomain-name-for-testing.test.local",
        "multi.tld.test.co",
        "123.test.local",
        "test-with-hyphens.test.local",
    ] {
        h.create_a(domain, "203.0.113.123").await;
        let response = h.query_a(domain).await;
        assert_eq!(rcode(&response), 0, "{} should resolve", domain);
    }
}

/// Every `RecordData` variant must round-trip from REST → LMDB → wire. Unit
/// tests cover the packet encoder in isolation and filter-level handler
/// tests cover serde, but nothing else proves the whole stack agrees on
/// type_code + lookup + rdata emission for each type.
#[tokio::test]
async fn all_record_types_round_trip() {
    let h = TestHarness::spawn().await;

    // (POST body, qtype to dig)
    let cases: Vec<(serde_json::Value, u16)> = vec![
        (
            json!({"name": "a.rt.test", "ttl": 300, "class": "IN", "type": "A", "ip": "1.2.3.4"}),
            1,
        ),
        (
            json!({"name": "aaaa.rt.test", "ttl": 300, "class": "IN", "type": "AAAA", "ip": "2001:db8::1"}),
            28,
        ),
        (
            json!({"name": "cname.rt.test", "ttl": 300, "class": "IN", "type": "CNAME", "target": "target.rt.test"}),
            5,
        ),
        (
            json!({"name": "ptr.rt.test", "ttl": 300, "class": "IN", "type": "PTR", "target": "target.rt.test"}),
            12,
        ),
        (
            json!({"name": "ns.rt.test", "ttl": 300, "class": "IN", "type": "NS", "target": "ns1.rt.test"}),
            2,
        ),
        (
            json!({"name": "mx.rt.test", "ttl": 300, "class": "IN", "type": "MX", "preference": 10, "exchange": "mx1.rt.test"}),
            15,
        ),
        (
            json!({"name": "txt.rt.test", "ttl": 300, "class": "IN", "type": "TXT", "strings": ["v=spf1 -all"]}),
            16,
        ),
    ];

    for (body, qtype) in cases {
        let name = body["name"].as_str().unwrap().to_string();
        let type_name = body["type"].as_str().unwrap().to_string();

        let resp = h.post_record(body).await;
        assert!(
            resp.status().is_success(),
            "POST {} failed: {}",
            type_name,
            resp.status()
        );

        let response = h.query(&name, qtype).await;
        assert_eq!(rcode(&response), 0, "{} query should NOERROR", type_name);
        assert!(
            ancount(&response) >= 1,
            "{} query should return at least one answer (ancount={})",
            type_name,
            ancount(&response)
        );
    }
}

#[tokio::test]
async fn nxdomain_for_unknown_name() {
    let h = TestHarness::spawn().await;
    let response = h.query_a("definitely-not-there.test").await;
    assert_eq!(rcode(&response), 3, "expected NXDOMAIN");
    assert_eq!(ancount(&response), 0);
}

#[tokio::test]
async fn nodata_when_name_exists_but_type_does_not() {
    let h = TestHarness::spawn().await;
    // Name has A only; asking for AAAA must return NOERROR with zero answers.
    h.create_a("only-a.test", "1.2.3.4").await;

    let response = h.query("only-a.test", 28).await;
    assert_eq!(rcode(&response), 0, "NODATA is signalled by NOERROR rcode");
    assert_eq!(ancount(&response), 0, "NODATA has no answers");
}

#[tokio::test]
async fn ns_rrset_returns_all_answers_on_wire() {
    let h = TestHarness::spawn().await;
    let name = "delegated.test";

    for target in ["ns1.delegated.test", "ns2.delegated.test"] {
        let resp = h
            .post_record(json!({
                "name": name,
                "ttl": 300,
                "class": "IN",
                "type": "NS",
                "target": target,
            }))
            .await;
        assert!(
            resp.status().is_success(),
            "NS POST failed: {}",
            resp.status()
        );
    }

    let response = h.query(name, 2).await;
    assert_eq!(rcode(&response), 0);
    assert_eq!(
        ancount(&response),
        2,
        "both NS records should be in the answer section"
    );
}

/// The full CRUD → DNS lifecycle: PUT updates the served value, DELETE
/// removes it so subsequent queries NXDOMAIN. Scans the raw response for the
/// new IP bytes to prove the update actually changed what's on the wire —
/// without that, the test couldn't distinguish "PUT succeeded" from "PUT
/// no-op'd but the record still resolves to the old value".
#[tokio::test]
async fn crud_lifecycle_reaches_the_wire() {
    let h = TestHarness::spawn().await;

    let id = h.create_a("lifecycle.test", "1.2.3.4").await;
    let response = h.query_a("lifecycle.test").await;
    assert_eq!(rcode(&response), 0);
    assert!(
        response.windows(4).any(|w| w == [1, 2, 3, 4]),
        "initial A rdata should appear on the wire"
    );

    // Replace the payload wholesale via the nested `data` shape.
    let resp = h
        .put_record(
            &id,
            json!({
                "data": {"type": "A", "ip": "9.9.9.9"}
            }),
        )
        .await;
    assert!(resp.status().is_success(), "PUT failed: {}", resp.status());

    let response = h.query_a("lifecycle.test").await;
    assert_eq!(rcode(&response), 0);
    assert!(
        response.windows(4).any(|w| w == [9, 9, 9, 9]),
        "updated A rdata should replace the old value on the wire"
    );
    assert!(
        !response.windows(4).any(|w| w == [1, 2, 3, 4]),
        "old A rdata must not linger"
    );

    let resp = h.delete_record(&id).await;
    assert!(
        resp.status().is_success(),
        "DELETE failed: {}",
        resp.status()
    );

    let response = h.query_a("lifecycle.test").await;
    assert_eq!(
        rcode(&response),
        3,
        "deleted record should NXDOMAIN on query"
    );
}

/// RFC 2181 §10.1: a CNAME cannot coexist with any other record type at the
/// same name. The write path rejects both directions (CNAME-then-A and
/// A-then-CNAME) with 409.
#[tokio::test]
async fn cname_exclusivity_enforced_on_both_directions() {
    let h = TestHarness::spawn().await;

    // CNAME first, then A at same name -> 409
    let resp = h
        .post_record(json!({
            "name": "cname-first.test",
            "ttl": 300,
            "class": "IN",
            "type": "CNAME",
            "target": "elsewhere.test",
        }))
        .await;
    assert!(resp.status().is_success());

    let resp = h
        .post_record(json!({
            "name": "cname-first.test",
            "ttl": 300,
            "class": "IN",
            "type": "A",
            "ip": "1.2.3.4",
        }))
        .await;
    assert_eq!(resp.status().as_u16(), 409, "A-over-CNAME must 409");

    // A first, then CNAME at same name -> 409
    let resp = h
        .post_record(json!({
            "name": "a-first.test",
            "ttl": 300,
            "class": "IN",
            "type": "A",
            "ip": "1.2.3.4",
        }))
        .await;
    assert!(resp.status().is_success());

    let resp = h
        .post_record(json!({
            "name": "a-first.test",
            "ttl": 300,
            "class": "IN",
            "type": "CNAME",
            "target": "elsewhere.test",
        }))
        .await;
    assert_eq!(resp.status().as_u16(), 409, "CNAME-over-A must 409");
}

#[tokio::test]
async fn sustained_load() {
    let h = TestHarness::spawn().await;
    let domain = "load.test.local";
    h.create_a(domain, "203.0.113.200").await;

    // Keep this short — CI boxes are slow. We care about "doesn't fall over",
    // not an absolute qps number.
    let duration = Duration::from_secs(1);
    let start = Instant::now();
    let mut queries = 0;
    let mut errors = 0;
    while start.elapsed() < duration {
        let resp = h.query_a(domain).await;
        if rcode(&resp) == 0 {
            queries += 1;
        } else {
            errors += 1;
        }
    }
    let error_rate = errors as f64 / (queries + errors) as f64 * 100.0;
    println!(
        "{} queries in {:?}, {:.2}% errors",
        queries,
        start.elapsed(),
        error_rate
    );
    assert!(queries > 100, "should get at least 100 queries in 1s");
    assert!(error_rate < 1.0, "error rate should be near zero");
}
