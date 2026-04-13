//! GET /records list handler tests. Run against a real in-process RIND
//! instance via `TestHarness`.

use serde_json::Value;

mod common;
use common::harness::TestHarness;

/// Seed three records with a small sleep between creates so server-assigned
/// `created_at` timestamps are strictly ordered (the list endpoint sorts
/// ascending by created_at).
async fn seed_three(h: &TestHarness) {
    let bodies = [
        serde_json::json!({
            "name": "example1.com", "ttl": 300, "class": "IN",
            "type": "A", "ip": "192.168.1.1"
        }),
        serde_json::json!({
            "name": "example2.com", "ttl": 600, "class": "IN",
            "type": "A", "ip": "192.168.1.2"
        }),
        serde_json::json!({
            "name": "example3.com", "ttl": 300, "class": "IN",
            "type": "AAAA", "ip": "2001:db8::1"
        }),
    ];
    for body in bodies {
        assert_eq!(h.post_record(body).await.status(), 201);
        // Microsecond timestamps + LMDB commit latency are usually enough to
        // produce monotonic created_at on Linux, but a tiny yield makes this
        // deterministic under heavy CI parallelism.
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
}

async fn get_records(h: &TestHarness, query: &str) -> reqwest::Response {
    h.http
        .get(format!("{}/records{}", h.api_base, query))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn test_list_records_default_pagination() {
    let h = TestHarness::spawn().await;
    seed_three(&h).await;

    let resp = get_records(&h, "").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert!(body["data"].is_object());
    assert!(body["error"].is_null());

    let data = &body["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["page"], 1);
    assert_eq!(data["per_page"], 50);
    let records = data["records"].as_array().unwrap();
    assert_eq!(records.len(), 3);
    // Oldest created_at first.
    assert_eq!(records[0]["name"], "example1.com");
    assert_eq!(records[1]["name"], "example2.com");
    assert_eq!(records[2]["name"], "example3.com");
}

#[tokio::test]
async fn test_list_records_with_pagination() {
    let h = TestHarness::spawn().await;
    seed_three(&h).await;

    let resp = get_records(&h, "?page=1&per_page=2").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let data = &body["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["page"], 1);
    assert_eq!(data["per_page"], 2);
    assert_eq!(data["records"].as_array().unwrap().len(), 2);

    let resp = get_records(&h, "?page=2&per_page=2").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let data = &body["data"];
    assert_eq!(data["page"], 2);
    assert_eq!(data["records"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_list_records_empty_page() {
    let h = TestHarness::spawn().await;
    seed_three(&h).await;

    let resp = get_records(&h, "?page=10&per_page=10").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let data = &body["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["records"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_records_invalid_pagination() {
    let h = TestHarness::spawn().await;
    seed_three(&h).await;

    let resp = get_records(&h, "?page=0&per_page=10").await;
    assert_eq!(resp.status(), 400);

    let resp = get_records(&h, "?page=1&per_page=2000").await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_list_records_malformed_query_params() {
    let h = TestHarness::spawn().await;
    seed_three(&h).await;

    let resp = get_records(&h, "?page=invalid&per_page=10").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["page"], 1);
    assert_eq!(body["data"]["per_page"], 10);

    let resp = get_records(&h, "?page=1&per_page=invalid").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["per_page"], 50);
}

#[tokio::test]
async fn test_list_records_record_structure() {
    let h = TestHarness::spawn().await;
    seed_three(&h).await;

    let resp = get_records(&h, "?per_page=10").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let records = body["data"]["records"].as_array().unwrap();

    for record in records {
        assert!(record["id"].is_string());
        assert!(record["name"].is_string());
        assert!(record["ttl"].is_number());
        assert!(record["class"].is_string());
        assert!(record["type"].is_string());
        assert!(record["ip"].is_string());
        assert!(record["created_at"].is_string());
        assert!(record["updated_at"].is_string());
    }

    let types: Vec<&str> = records
        .iter()
        .map(|r| r["type"].as_str().unwrap())
        .collect();
    assert!(types.contains(&"A"));
    assert!(types.contains(&"AAAA"));
}
