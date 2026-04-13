//! REST PUT handler tests. Run against a real in-process RIND instance via
//! `TestHarness`.

use rind::update::{ApiResponse, DnsRecord};

mod common;
use common::harness::TestHarness;

#[tokio::test]
async fn test_put_record_change_payload_to_aaaa() {
    let h = TestHarness::spawn().await;
    let id = h.create_a("test.example.com", "192.168.1.1").await;

    // Swap the A record for an AAAA by replacing `data` wholesale.
    let resp = h
        .put_record(
            &id,
            serde_json::json!({
                "name": "updated.example.com",
                "ttl": 600,
                "data": { "type": "AAAA", "ip": "2001:db8::1" }
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: ApiResponse<DnsRecord> = resp.json().await.unwrap();
    let updated = body.data.unwrap();
    assert_eq!(updated.name, "updated.example.com");
    assert_eq!(updated.ttl, 600);
    assert_eq!(updated.data.type_name(), "AAAA");
}

#[tokio::test]
async fn test_put_record_not_found() {
    let h = TestHarness::spawn().await;
    let resp = h
        .put_record(
            "non-existent-id",
            serde_json::json!({ "name": "updated.example.com" }),
        )
        .await;
    assert_eq!(resp.status(), 404);
    let body: ApiResponse<DnsRecord> = resp.json().await.unwrap();
    assert!(body.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_put_record_partial_update_ttl_only() {
    let h = TestHarness::spawn().await;
    let id = h.create_a("test.example.com", "192.168.1.1").await;

    // Only TTL changes — no `data` field, so the A record payload is preserved.
    let resp = h.put_record(&id, serde_json::json!({ "ttl": 900 })).await;
    assert_eq!(resp.status(), 200);
    let body: ApiResponse<DnsRecord> = resp.json().await.unwrap();
    let updated = body.data.unwrap();
    assert_eq!(updated.name, "test.example.com");
    assert_eq!(updated.ttl, 900);
    assert_eq!(updated.data.type_name(), "A");
}
