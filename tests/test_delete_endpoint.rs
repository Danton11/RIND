//! REST DELETE handler tests. Run against a real in-process RIND instance via
//! `TestHarness`.

use rind::update::ApiResponse;

mod common;
use common::harness::TestHarness;

#[tokio::test]
async fn test_delete_record_success() {
    let h = TestHarness::spawn().await;
    let id = h.create_a("test.example.com", "192.168.1.1").await;

    // GET returns the record before delete.
    let resp = h
        .http
        .get(format!("{}/records/{}", h.api_base, id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = h.delete_record(&id).await;
    assert_eq!(resp.status(), 204);
    // RFC 7230 §3.3.3 — 204 responses carry no body.
    assert!(resp.content_length().unwrap_or(0) == 0);

    let resp = h
        .http
        .get(format!("{}/records/{}", h.api_base, id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_delete_record_not_found() {
    let h = TestHarness::spawn().await;
    let resp = h.delete_record("non-existent-id").await;
    assert_eq!(resp.status(), 404);
    let body: ApiResponse<()> = resp.json().await.unwrap();
    assert!(!body.success);
    assert!(body.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_delete_record_multiple_records() {
    let h = TestHarness::spawn().await;
    let id1 = h.create_a("test1.example.com", "192.168.1.1").await;
    let id2 = h.create_a("test2.example.com", "192.168.1.2").await;

    assert_eq!(h.delete_record(&id1).await.status(), 204);

    // id1 is gone, id2 is still there.
    let r = h
        .http
        .get(format!("{}/records/{}", h.api_base, id1))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
    let r = h
        .http
        .get(format!("{}/records/{}", h.api_base, id2))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);

    assert_eq!(h.delete_record(&id2).await.status(), 204);
    let r = h
        .http
        .get(format!("{}/records/{}", h.api_base, id2))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
}

#[tokio::test]
async fn test_delete_record_idempotent() {
    let h = TestHarness::spawn().await;
    let id = h.create_a("test.example.com", "192.168.1.1").await;

    assert_eq!(h.delete_record(&id).await.status(), 204);

    let resp = h.delete_record(&id).await;
    assert_eq!(resp.status(), 404);
    let body: ApiResponse<()> = resp.json().await.unwrap();
    assert!(body.error.unwrap().contains("not found"));
}
