//! REST POST/PUT handler tests. Run against a real in-process RIND instance
//! via `TestHarness` — real sockets, real warp filters, real LMDB tempdir.

use rind::update::{ApiResponse, DnsRecord, RecordData};

mod common;
use common::harness::TestHarness;

async fn created_record(resp: reqwest::Response) -> DnsRecord {
    assert_eq!(resp.status(), 201);
    let body: ApiResponse<DnsRecord> = resp.json().await.unwrap();
    assert!(body.success);
    body.data.unwrap()
}

async fn post_and_get_id(h: &TestHarness, body: serde_json::Value) -> String {
    created_record(h.post_record(body).await).await.id
}

#[tokio::test]
async fn test_post_a_record_success() {
    let h = TestHarness::spawn().await;
    let resp = h
        .post_record(serde_json::json!({
            "name": "test.example.com",
            "ttl": 600,
            "class": "IN",
            "type": "A",
            "ip": "192.168.1.1"
        }))
        .await;
    let created = created_record(resp).await;
    assert_eq!(created.name, "test.example.com");
    assert_eq!(created.ttl, 600);
    assert_eq!(created.class, "IN");
    assert_eq!(
        created.data,
        RecordData::A {
            ip: "192.168.1.1".parse().unwrap()
        }
    );
}

#[tokio::test]
async fn test_post_aaaa_record_success() {
    let h = TestHarness::spawn().await;
    let resp = h
        .post_record(serde_json::json!({
            "name": "v6.example.com",
            "type": "AAAA",
            "ip": "2001:db8::1"
        }))
        .await;
    let created = created_record(resp).await;
    assert_eq!(created.ttl, 300); // default
    assert_eq!(created.class, "IN"); // default
    assert_eq!(created.data.type_name(), "AAAA");
}

#[tokio::test]
async fn test_post_record_invalid_ip_rejected_at_parse() {
    let h = TestHarness::spawn().await;
    // Malformed IP — serde rejects at the Ipv4Addr parse step before we reach
    // the handler, so warp returns 400 from body::json().
    let resp = h
        .post_record(serde_json::json!({
            "name": "test.example.com",
            "type": "A",
            "ip": "not-an-ip"
        }))
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_post_record_duplicate_error() {
    let h = TestHarness::spawn().await;
    let body = serde_json::json!({
        "name": "duplicate.example.com",
        "type": "A",
        "ip": "192.168.1.1"
    });
    assert_eq!(h.post_record(body.clone()).await.status(), 201);

    let resp = h
        .post_record(serde_json::json!({
            "name": "duplicate.example.com",
            "type": "A",
            "ip": "10.0.0.1"
        }))
        .await;
    assert_eq!(resp.status(), 409);
    let body: ApiResponse<DnsRecord> = resp.json().await.unwrap();
    assert!(body.error.unwrap().contains("already exists"));
}

#[tokio::test]
async fn test_post_record_empty_name_validation() {
    let h = TestHarness::spawn().await;
    let resp = h
        .post_record(serde_json::json!({
            "name": "",
            "type": "A",
            "ip": "192.168.1.1"
        }))
        .await;
    assert_eq!(resp.status(), 400);
    let body: ApiResponse<DnsRecord> = resp.json().await.unwrap();
    assert!(body.error.unwrap().contains("Missing required field"));
}

#[tokio::test]
async fn test_post_cname_record_success() {
    let h = TestHarness::spawn().await;
    let resp = h
        .post_record(serde_json::json!({
            "name": "alias.example.com",
            "type": "CNAME",
            "target": "canonical.example.com"
        }))
        .await;
    let created = created_record(resp).await;
    assert_eq!(
        created.data,
        RecordData::Cname {
            target: "canonical.example.com".to_string()
        }
    );
}

#[tokio::test]
async fn test_post_cname_conflicts_with_existing_a_record() {
    let h = TestHarness::spawn().await;

    // First: plain A record at foo.example.com.
    assert_eq!(
        h.post_record(serde_json::json!({
            "name": "foo.example.com",
            "type": "A",
            "ip": "192.168.1.1"
        }))
        .await
        .status(),
        201
    );

    // Then: CNAME at same name. RFC 2181 §10.1 → 409.
    let resp = h
        .post_record(serde_json::json!({
            "name": "foo.example.com",
            "type": "CNAME",
            "target": "other.example.com"
        }))
        .await;
    assert_eq!(resp.status(), 409);
    let body: ApiResponse<DnsRecord> = resp.json().await.unwrap();
    assert!(body.error.unwrap().contains("CNAME"));
}

#[tokio::test]
async fn test_post_a_conflicts_with_existing_cname() {
    let h = TestHarness::spawn().await;

    assert_eq!(
        h.post_record(serde_json::json!({
            "name": "alias.example.com",
            "type": "CNAME",
            "target": "canonical.example.com"
        }))
        .await
        .status(),
        201
    );

    let resp = h
        .post_record(serde_json::json!({
            "name": "alias.example.com",
            "type": "A",
            "ip": "192.168.1.1"
        }))
        .await;
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn test_post_ptr_record_success() {
    let h = TestHarness::spawn().await;
    let resp = h
        .post_record(serde_json::json!({
            "name": "1.1.168.192.in-addr.arpa",
            "type": "PTR",
            "target": "host.example.com"
        }))
        .await;
    let created = created_record(resp).await;
    assert_eq!(
        created.data,
        RecordData::Ptr {
            target: "host.example.com".to_string()
        }
    );
}

#[tokio::test]
async fn test_post_ns_records_multi_value_allowed() {
    let h = TestHarness::spawn().await;
    // Two NS records at the same name — delegation set. Both should succeed.
    assert_eq!(
        h.post_record(serde_json::json!({
            "name": "example.com",
            "type": "NS",
            "target": "ns1.example.com"
        }))
        .await
        .status(),
        201
    );
    assert_eq!(
        h.post_record(serde_json::json!({
            "name": "example.com",
            "type": "NS",
            "target": "ns2.example.com"
        }))
        .await
        .status(),
        201
    );
}

#[tokio::test]
async fn test_post_ns_rrset_rejects_exact_duplicate_rdata() {
    let h = TestHarness::spawn().await;
    // RFC 2181 §5: an RRSet is a set, not a bag. Inserting the same NS rdata
    // at the same name twice must be rejected.
    let ns = serde_json::json!({
        "name": "example.com",
        "type": "NS",
        "target": "ns1.example.com"
    });
    assert_eq!(h.post_record(ns.clone()).await.status(), 201);
    assert_eq!(h.post_record(ns).await.status(), 409);
}

#[tokio::test]
async fn test_post_mx_records_multi_value_and_ordering() {
    let h = TestHarness::spawn().await;
    assert_eq!(
        h.post_record(serde_json::json!({
            "name": "example.com",
            "type": "MX",
            "preference": 10,
            "exchange": "mail1.example.com"
        }))
        .await
        .status(),
        201
    );
    assert_eq!(
        h.post_record(serde_json::json!({
            "name": "example.com",
            "type": "MX",
            "preference": 20,
            "exchange": "mail2.example.com"
        }))
        .await
        .status(),
        201
    );
}

#[tokio::test]
async fn test_post_mx_same_preference_different_exchange_allowed() {
    // Two MX records with the *same* preference but different exchanges is
    // how round-robin MX works. Must not be rejected as a duplicate.
    let h = TestHarness::spawn().await;
    assert_eq!(
        h.post_record(serde_json::json!({
            "name": "example.com",
            "type": "MX",
            "preference": 10,
            "exchange": "mail1.example.com"
        }))
        .await
        .status(),
        201
    );
    assert_eq!(
        h.post_record(serde_json::json!({
            "name": "example.com",
            "type": "MX",
            "preference": 10,
            "exchange": "mail2.example.com"
        }))
        .await
        .status(),
        201
    );
}

#[tokio::test]
async fn test_post_txt_record_success() {
    let h = TestHarness::spawn().await;
    let resp = h
        .post_record(serde_json::json!({
            "name": "txt.example.com",
            "type": "TXT",
            "strings": ["v=spf1 -all"]
        }))
        .await;
    let created = created_record(resp).await;
    assert_eq!(
        created.data,
        RecordData::Txt {
            strings: vec!["v=spf1 -all".to_string()]
        }
    );
}

#[tokio::test]
async fn test_post_txt_rejects_empty_strings_vec() {
    let h = TestHarness::spawn().await;
    let resp = h
        .post_record(serde_json::json!({
            "name": "txt.example.com",
            "type": "TXT",
            "strings": []
        }))
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_post_txt_rejects_oversized_string() {
    // RFC 1035 §3.3.14 hard-caps each character-string at 255 bytes. Users
    // must split longer values into multiple Vec entries manually.
    let h = TestHarness::spawn().await;
    let oversized = "a".repeat(256);
    let resp = h
        .post_record(serde_json::json!({
            "name": "txt.example.com",
            "type": "TXT",
            "strings": [oversized]
        }))
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_post_txt_accepts_multiple_strings() {
    // A TXT record with multiple character-strings in a single Vec — each
    // string gets its own length-prefixed octet sequence on the wire.
    let h = TestHarness::spawn().await;
    let resp = h
        .post_record(serde_json::json!({
            "name": "txt.example.com",
            "type": "TXT",
            "strings": ["part-one", "part-two", "part-three"]
        }))
        .await;
    assert_eq!(resp.status(), 201);
}

#[tokio::test]
async fn test_post_record_missing_type_is_rejected() {
    let h = TestHarness::spawn().await;
    // No "type" field — internally-tagged enum can't deserialize, 400.
    let resp = h
        .post_record(serde_json::json!({
            "name": "no-type.example.com",
            "ip": "1.2.3.4"
        }))
        .await;
    assert_eq!(resp.status(), 400);
}

// ---------- PUT / update tests for new record types ----------

#[tokio::test]
async fn test_put_cname_target_update() {
    let h = TestHarness::spawn().await;
    let id = post_and_get_id(
        &h,
        serde_json::json!({
            "name": "alias.example.com",
            "type": "CNAME",
            "target": "old.example.com"
        }),
    )
    .await;

    let resp = h
        .put_record(
            &id,
            serde_json::json!({
                "data": { "type": "CNAME", "target": "new.example.com" }
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: ApiResponse<DnsRecord> = resp.json().await.unwrap();
    assert_eq!(
        body.data.unwrap().data,
        RecordData::Cname {
            target: "new.example.com".to_string()
        }
    );
}

#[tokio::test]
async fn test_put_ptr_target_update() {
    let h = TestHarness::spawn().await;
    let id = post_and_get_id(
        &h,
        serde_json::json!({
            "name": "1.1.1.10.in-addr.arpa",
            "type": "PTR",
            "target": "host1.example.com"
        }),
    )
    .await;

    let resp = h
        .put_record(
            &id,
            serde_json::json!({
                "data": { "type": "PTR", "target": "host2.example.com" }
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: ApiResponse<DnsRecord> = resp.json().await.unwrap();
    assert_eq!(
        body.data.unwrap().data,
        RecordData::Ptr {
            target: "host2.example.com".to_string()
        }
    );
}

#[tokio::test]
async fn test_put_ns_rdata_rewrite_succeeds() {
    let h = TestHarness::spawn().await;
    let id1 = post_and_get_id(
        &h,
        serde_json::json!({
            "name": "example.com",
            "type": "NS",
            "target": "ns1.example.com"
        }),
    )
    .await;
    let _id2 = post_and_get_id(
        &h,
        serde_json::json!({
            "name": "example.com",
            "type": "NS",
            "target": "ns2.example.com"
        }),
    )
    .await;

    // Rewrite id1 to a fresh, non-duplicate target — different rdata from id2.
    let resp = h
        .put_record(
            &id1,
            serde_json::json!({
                "data": { "type": "NS", "target": "ns3.example.com" }
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_put_ns_rrset_duplicate_rejected() {
    let h = TestHarness::spawn().await;
    let id1 = post_and_get_id(
        &h,
        serde_json::json!({
            "name": "example.com",
            "type": "NS",
            "target": "ns1.example.com"
        }),
    )
    .await;
    let _id2 = post_and_get_id(
        &h,
        serde_json::json!({
            "name": "example.com",
            "type": "NS",
            "target": "ns2.example.com"
        }),
    )
    .await;

    // Change id1 to match id2's rdata — RFC 2181 §5 violation.
    let resp = h
        .put_record(
            &id1,
            serde_json::json!({
                "data": { "type": "NS", "target": "ns2.example.com" }
            }),
        )
        .await;
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn test_put_mx_preference_update() {
    let h = TestHarness::spawn().await;
    let id = post_and_get_id(
        &h,
        serde_json::json!({
            "name": "example.com",
            "type": "MX",
            "preference": 10,
            "exchange": "mail.example.com"
        }),
    )
    .await;

    let resp = h
        .put_record(
            &id,
            serde_json::json!({
                "data": { "type": "MX", "preference": 20, "exchange": "mail.example.com" }
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: ApiResponse<DnsRecord> = resp.json().await.unwrap();
    assert_eq!(
        body.data.unwrap().data,
        RecordData::Mx {
            preference: 20,
            exchange: "mail.example.com".to_string()
        }
    );
}

#[tokio::test]
async fn test_put_txt_strings_update() {
    let h = TestHarness::spawn().await;
    let id = post_and_get_id(
        &h,
        serde_json::json!({
            "name": "example.com",
            "type": "TXT",
            "strings": ["v=spf1 -all"]
        }),
    )
    .await;

    let resp = h
        .put_record(
            &id,
            serde_json::json!({
                "data": { "type": "TXT", "strings": ["v=spf1 include:_spf.example.com -all"] }
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: ApiResponse<DnsRecord> = resp.json().await.unwrap();
    assert_eq!(
        body.data.unwrap().data,
        RecordData::Txt {
            strings: vec!["v=spf1 include:_spf.example.com -all".to_string()]
        }
    );
}

#[tokio::test]
async fn test_put_cname_exclusivity_enforced_on_rename() {
    let h = TestHarness::spawn().await;
    // Plain A record.
    let _a_id = post_and_get_id(
        &h,
        serde_json::json!({
            "name": "foo.example.com",
            "type": "A",
            "ip": "1.2.3.4"
        }),
    )
    .await;

    // CNAME at a different name — rename onto the A's name and expect 409.
    let cname_id = post_and_get_id(
        &h,
        serde_json::json!({
            "name": "other.example.com",
            "type": "CNAME",
            "target": "bar.example.com"
        }),
    )
    .await;

    let resp = h
        .put_record(&cname_id, serde_json::json!({ "name": "foo.example.com" }))
        .await;
    assert_eq!(resp.status(), 409);
}
