use std::sync::Arc;
use tokio::sync::RwLock;
use warp::test::request;
use warp::Filter;
use serde_json::Value;
use std::collections::HashMap;

// Import the modules we need to test
use rind::update::{DnsRecord, DnsRecords, ApiResponse, RecordListResponse};

/// Helper function to create test records
fn create_test_records() -> Arc<RwLock<DnsRecords>> {
    let mut records = HashMap::new();
    
    // Create test records with different timestamps to test ordering
    let now = chrono::Utc::now();
    
    let record1 = DnsRecord {
        id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
        name: "example1.com".to_string(),
        ip: Some("192.168.1.1".parse().unwrap()),
        ttl: 300,
        record_type: "A".to_string(),
        class: "IN".to_string(),
        value: None,
        created_at: now - chrono::Duration::minutes(10),
        updated_at: now - chrono::Duration::minutes(10),
    };
    
    let record2 = DnsRecord {
        id: "550e8400-e29b-41d4-a716-446655440002".to_string(),
        name: "example2.com".to_string(),
        ip: Some("192.168.1.2".parse().unwrap()),
        ttl: 600,
        record_type: "A".to_string(),
        class: "IN".to_string(),
        value: None,
        created_at: now - chrono::Duration::minutes(5),
        updated_at: now - chrono::Duration::minutes(5),
    };
    
    let record3 = DnsRecord {
        id: "550e8400-e29b-41d4-a716-446655440003".to_string(),
        name: "example3.com".to_string(),
        ip: None,
        ttl: 300,
        record_type: "CNAME".to_string(),
        class: "IN".to_string(),
        value: Some("target.example.com".to_string()),
        created_at: now,
        updated_at: now,
    };
    
    records.insert(record1.id.clone(), record1);
    records.insert(record2.id.clone(), record2);
    records.insert(record3.id.clone(), record3);
    
    Arc::new(RwLock::new(records))
}

// Handler for GET /records endpoint with pagination (same as main.rs)
async fn list_records_handler(
    query_params: std::collections::HashMap<String, String>,
    records: Arc<RwLock<DnsRecords>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    // Parse pagination parameters with defaults
    let page = query_params
        .get("page")
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(1);
    
    let per_page = query_params
        .get("per_page")
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(50);

    match rind::update::list_records(records, page, per_page).await {
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
                500 => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            Ok(warp::reply::with_status(
                warp::reply::json(&response),
                status_code,
            ))
        }
    }
}

/// Create a test API filter similar to main.rs
fn create_test_api() -> impl warp::Filter<Extract = impl warp::Reply> + Clone {
    let records = create_test_records();
    let records_filter = warp::any().map(move || Arc::clone(&records));
    
    // GET /records - List all records with pagination
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
    
    let response = request()
        .method("GET")
        .path("/records")
        .reply(&api)
        .await;
    
    assert_eq!(response.status(), 200);
    
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    
    // Verify response structure
    assert_eq!(body["success"], true);
    assert!(body["data"].is_object());
    assert!(body["error"].is_null());
    assert!(body["timestamp"].is_string());
    
    // Verify pagination metadata
    let data = &body["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["page"], 1);
    assert_eq!(data["per_page"], 50);
    assert_eq!(data["records"].as_array().unwrap().len(), 3);
    
    // Verify records are sorted by creation time (oldest first)
    let records = data["records"].as_array().unwrap();
    assert_eq!(records[0]["name"], "example1.com");
    assert_eq!(records[1]["name"], "example2.com");
    assert_eq!(records[2]["name"], "example3.com");
}

#[tokio::test]
async fn test_list_records_with_pagination() {
    let api = create_test_api();
    
    // Test first page with per_page=2
    let response = request()
        .method("GET")
        .path("/records?page=1&per_page=2")
        .reply(&api)
        .await;
    
    assert_eq!(response.status(), 200);
    
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    
    // Verify response structure
    assert_eq!(body["success"], true);
    let data = &body["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["page"], 1);
    assert_eq!(data["per_page"], 2);
    assert_eq!(data["records"].as_array().unwrap().len(), 2);
    
    // Test second page
    let response = request()
        .method("GET")
        .path("/records?page=2&per_page=2")
        .reply(&api)
        .await;
    
    assert_eq!(response.status(), 200);
    
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    let data = &body["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["page"], 2);
    assert_eq!(data["per_page"], 2);
    assert_eq!(data["records"].as_array().unwrap().len(), 1); // Only 1 record on page 2
}

#[tokio::test]
async fn test_list_records_empty_page() {
    let api = create_test_api();
    
    // Test page beyond available records
    let response = request()
        .method("GET")
        .path("/records?page=10&per_page=10")
        .reply(&api)
        .await;
    
    assert_eq!(response.status(), 200);
    
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    
    // Verify response structure
    assert_eq!(body["success"], true);
    let data = &body["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["page"], 10);
    assert_eq!(data["per_page"], 10);
    assert_eq!(data["records"].as_array().unwrap().len(), 0); // No records on page 10
}

#[tokio::test]
async fn test_list_records_invalid_pagination() {
    let api = create_test_api();
    
    // Test invalid page number (0)
    let response = request()
        .method("GET")
        .path("/records?page=0&per_page=10")
        .reply(&api)
        .await;
    
    assert_eq!(response.status(), 400);
    
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"].is_string());
    
    // Test invalid per_page (too large)
    let response = request()
        .method("GET")
        .path("/records?page=1&per_page=2000")
        .reply(&api)
        .await;
    
    assert_eq!(response.status(), 400);
    
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn test_list_records_malformed_query_params() {
    let api = create_test_api();
    
    // Test with malformed page parameter (should default to 1)
    let response = request()
        .method("GET")
        .path("/records?page=invalid&per_page=10")
        .reply(&api)
        .await;
    
    assert_eq!(response.status(), 200);
    
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    let data = &body["data"];
    assert_eq!(data["page"], 1); // Should default to 1
    assert_eq!(data["per_page"], 10);
    
    // Test with malformed per_page parameter (should default to 50)
    let response = request()
        .method("GET")
        .path("/records?page=1&per_page=invalid")
        .reply(&api)
        .await;
    
    assert_eq!(response.status(), 200);
    
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    let data = &body["data"];
    assert_eq!(data["page"], 1);
    assert_eq!(data["per_page"], 50); // Should default to 50
}

#[tokio::test]
async fn test_list_records_record_structure() {
    let api = create_test_api();
    
    let response = request()
        .method("GET")
        .path("/records?per_page=1")
        .reply(&api)
        .await;
    
    assert_eq!(response.status(), 200);
    
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    let records = body["data"]["records"].as_array().unwrap();
    let record = &records[0];
    
    // Verify all required fields are present
    assert!(record["id"].is_string());
    assert!(record["name"].is_string());
    assert!(record["ttl"].is_number());
    assert!(record["record_type"].is_string());
    assert!(record["class"].is_string());
    assert!(record["created_at"].is_string());
    assert!(record["updated_at"].is_string());
    
    // IP and value can be null for certain record types
    assert!(record["ip"].is_string() || record["ip"].is_null());
    assert!(record["value"].is_string() || record["value"].is_null());
}