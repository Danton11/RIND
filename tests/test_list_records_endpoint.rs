use serde_json::Value;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::test::request;
use warp::Filter;

use rind::update::{ApiResponse, DnsRecord, DnsRecords, RecordData, RecordListResponse};

/// Build three records with staggered created_at timestamps so we can verify ordering.
fn create_test_records() -> Arc<RwLock<DnsRecords>> {
    let mut records = DnsRecords::new();
    let now = chrono::Utc::now();

    let mut record1 = DnsRecord::new(
        "example1.com".to_string(),
        300,
        "IN".to_string(),
        RecordData::A {
            ip: Ipv4Addr::new(192, 168, 1, 1),
        },
    );
    record1.created_at = now - chrono::Duration::minutes(10);
    record1.updated_at = record1.created_at;

    let mut record2 = DnsRecord::new(
        "example2.com".to_string(),
        600,
        "IN".to_string(),
        RecordData::A {
            ip: Ipv4Addr::new(192, 168, 1, 2),
        },
    );
    record2.created_at = now - chrono::Duration::minutes(5);
    record2.updated_at = record2.created_at;

    let mut record3 = DnsRecord::new(
        "example3.com".to_string(),
        300,
        "IN".to_string(),
        RecordData::Aaaa {
            ip: "2001:db8::1".parse().unwrap(),
        },
    );
    record3.created_at = now;
    record3.updated_at = now;

    records.insert(record1.id.clone(), record1);
    records.insert(record2.id.clone(), record2);
    records.insert(record3.id.clone(), record3);

    Arc::new(RwLock::new(records))
}

async fn list_records_handler(
    query_params: std::collections::HashMap<String, String>,
    records: Arc<RwLock<DnsRecords>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let page = query_params
        .get("page")
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(1);
    let per_page = query_params
        .get("per_page")
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(50);

    match rind::update::list_records(records, page, per_page, None).await {
        Ok(record_list) => {
            let response = ApiResponse::success(record_list);
            Ok(warp::reply::with_status(
                warp::reply::json(&response),
                warp::http::StatusCode::OK,
            ))
        }
        Err(e) => {
            let response = ApiResponse::<RecordListResponse>::error(e.to_string());
            let status_code = match e.to_status_code() {
                404 => warp::http::StatusCode::NOT_FOUND,
                400 => warp::http::StatusCode::BAD_REQUEST,
                409 => warp::http::StatusCode::CONFLICT,
                _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            Ok(warp::reply::with_status(
                warp::reply::json(&response),
                status_code,
            ))
        }
    }
}

fn create_test_api() -> impl warp::Filter<Extract = impl warp::Reply> + Clone {
    let records = create_test_records();
    let records_filter = warp::any().map(move || Arc::clone(&records));

    warp::path("records")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .and(records_filter)
        .and_then(list_records_handler)
}

#[tokio::test]
async fn test_list_records_default_pagination() {
    let api = create_test_api();
    let response = request().method("GET").path("/records").reply(&api).await;
    assert_eq!(response.status(), 200);

    let body: Value = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body["success"], true);
    assert!(body["data"].is_object());
    assert!(body["error"].is_null());

    let data = &body["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["page"], 1);
    assert_eq!(data["per_page"], 50);
    assert_eq!(data["records"].as_array().unwrap().len(), 3);

    // Oldest created_at first.
    let records = data["records"].as_array().unwrap();
    assert_eq!(records[0]["name"], "example1.com");
    assert_eq!(records[1]["name"], "example2.com");
    assert_eq!(records[2]["name"], "example3.com");
}

#[tokio::test]
async fn test_list_records_with_pagination() {
    let api = create_test_api();

    let response = request()
        .method("GET")
        .path("/records?page=1&per_page=2")
        .reply(&api)
        .await;
    assert_eq!(response.status(), 200);
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    let data = &body["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["page"], 1);
    assert_eq!(data["per_page"], 2);
    assert_eq!(data["records"].as_array().unwrap().len(), 2);

    let response = request()
        .method("GET")
        .path("/records?page=2&per_page=2")
        .reply(&api)
        .await;
    assert_eq!(response.status(), 200);
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    let data = &body["data"];
    assert_eq!(data["page"], 2);
    assert_eq!(data["records"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_list_records_empty_page() {
    let api = create_test_api();
    let response = request()
        .method("GET")
        .path("/records?page=10&per_page=10")
        .reply(&api)
        .await;
    assert_eq!(response.status(), 200);
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    let data = &body["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["records"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_records_invalid_pagination() {
    let api = create_test_api();

    let response = request()
        .method("GET")
        .path("/records?page=0&per_page=10")
        .reply(&api)
        .await;
    assert_eq!(response.status(), 400);

    let response = request()
        .method("GET")
        .path("/records?page=1&per_page=2000")
        .reply(&api)
        .await;
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_list_records_malformed_query_params() {
    let api = create_test_api();

    let response = request()
        .method("GET")
        .path("/records?page=invalid&per_page=10")
        .reply(&api)
        .await;
    assert_eq!(response.status(), 200);
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body["data"]["page"], 1);
    assert_eq!(body["data"]["per_page"], 10);

    let response = request()
        .method("GET")
        .path("/records?page=1&per_page=invalid")
        .reply(&api)
        .await;
    assert_eq!(response.status(), 200);
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body["data"]["per_page"], 50);
}

#[tokio::test]
async fn test_list_records_record_structure() {
    let api = create_test_api();
    let response = request()
        .method("GET")
        .path("/records?per_page=10")
        .reply(&api)
        .await;
    assert_eq!(response.status(), 200);

    let body: Value = serde_json::from_slice(response.body()).unwrap();
    let records = body["data"]["records"].as_array().unwrap();

    // Base metadata present on every record.
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

    // Expect at least one A and one AAAA among our seed data.
    let types: Vec<&str> = records
        .iter()
        .map(|r| r["type"].as_str().unwrap())
        .collect();
    assert!(types.contains(&"A"));
    assert!(types.contains(&"AAAA"));
}
