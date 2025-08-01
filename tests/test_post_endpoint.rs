use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;
use serde_json::json;
use std::net::Ipv4Addr;

use rind::update::{DnsRecord, DnsRecords, CreateRecordRequest, ApiResponse};

// Helper function to create a test server
async fn create_test_server() -> (Arc<RwLock<DnsRecords>>, impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone) {
    let records = DnsRecords::new();
    let records_arc = Arc::new(RwLock::new(records));
    let records_filter = warp::any().map({
        let records = Arc::clone(&records_arc);
        move || Arc::clone(&records)
    });

    // Handler for POST /records endpoint (copied from main.rs)
    async fn create_record_handler(
        create_request: CreateRecordRequest,
        records: Arc<RwLock<DnsRecords>>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        match rind::update::create_record_from_request(records, create_request, None).await {
            Ok(record) => {
                let response = ApiResponse::success(record);
                Ok(warp::reply::with_status(
                    warp::reply::json(&response),
                    warp::http::StatusCode::CREATED,
                ))
            }
            Err(e) => {
                let response = ApiResponse::<DnsRecord>::error(e.to_string());
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

    // Handler for GET /records/{id} endpoint (for verification)
    async fn get_record_handler(
        id: String,
        records: Arc<RwLock<DnsRecords>>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        match rind::update::get_record(records, &id, None).await {
            Ok(record) => {
                let response = ApiResponse::success(record);
                Ok(warp::reply::with_status(
                    warp::reply::json(&response),
                    warp::http::StatusCode::OK,
                ))
            }
            Err(e) => {
                let response = ApiResponse::<DnsRecord>::error(e.to_string());
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

    // POST /records - Create new record
    let create_record_route = warp::path("records")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(records_filter.clone())
        .and_then(create_record_handler);

    // GET /records/{id} - Retrieve a specific record by ID (for verification)
    let get_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::get())
        .and(records_filter.clone())
        .and_then(get_record_handler);

    let routes = create_record_route.or(get_record_route);
    
    (records_arc, routes)
}

#[tokio::test]
async fn test_post_record_success_with_all_fields() {
    let (records_arc, routes) = create_test_server().await;

    // Create record request with all fields
    let create_request = CreateRecordRequest {
        name: "test.example.com".to_string(),
        ip: Some("192.168.1.1".to_string()),
        ttl: Some(600),
        record_type: Some("A".to_string()),
        class: Some("IN".to_string()),
        value: None,
    };

    // Make POST request
    let response = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&create_request)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 201); // HTTP 201 Created
    
    // Parse response
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(body.success);
    assert!(body.data.is_some());
    
    let created_record = body.data.unwrap();
    assert_eq!(created_record.name, "test.example.com");
    assert_eq!(created_record.ip, Some(Ipv4Addr::new(192, 168, 1, 1)));
    assert_eq!(created_record.ttl, 600);
    assert_eq!(created_record.record_type, "A");
    assert_eq!(created_record.class, "IN");
    assert!(created_record.id.len() > 0); // Should have generated UUID
    
    // Verify record was actually stored
    let records = records_arc.read().await;
    assert_eq!(records.len(), 1);
    assert!(records.contains_key(&created_record.id));
}

#[tokio::test]
async fn test_post_record_success_with_defaults() {
    let (_records_arc, routes) = create_test_server().await;

    // Create record request with minimal fields (using defaults)
    let create_request = CreateRecordRequest {
        name: "minimal.example.com".to_string(),
        ip: Some("10.0.0.1".to_string()),
        ttl: None,        // Should default to 300
        record_type: None, // Should default to "A"
        class: None,      // Should default to "IN"
        value: None,
    };

    // Make POST request
    let response = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&create_request)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 201);
    
    // Parse response
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(body.success);
    
    let created_record = body.data.unwrap();
    assert_eq!(created_record.name, "minimal.example.com");
    assert_eq!(created_record.ip, Some(Ipv4Addr::new(10, 0, 0, 1)));
    assert_eq!(created_record.ttl, 300);      // Default TTL
    assert_eq!(created_record.record_type, "A"); // Default type
    assert_eq!(created_record.class, "IN");   // Default class
}

#[tokio::test]
async fn test_post_record_cname_with_value() {
    let (_records_arc, routes) = create_test_server().await;

    // Create CNAME record request
    let create_request = CreateRecordRequest {
        name: "www.example.com".to_string(),
        ip: None, // CNAME records don't have IP
        ttl: Some(300),
        record_type: Some("CNAME".to_string()),
        class: Some("IN".to_string()),
        value: Some("example.com".to_string()),
    };

    // Make POST request
    let response = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&create_request)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 201);
    
    // Parse response
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(body.success);
    
    let created_record = body.data.unwrap();
    assert_eq!(created_record.name, "www.example.com");
    assert_eq!(created_record.ip, None);
    assert_eq!(created_record.record_type, "CNAME");
    assert_eq!(created_record.value, Some("example.com".to_string()));
}

#[tokio::test]
async fn test_post_record_validation_error_invalid_ip() {
    let (_records_arc, routes) = create_test_server().await;

    // Create record request with invalid IP
    let create_request = CreateRecordRequest {
        name: "test.example.com".to_string(),
        ip: Some("invalid-ip-address".to_string()),
        ttl: None,
        record_type: None,
        class: None,
        value: None,
    };

    // Make POST request
    let response = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&create_request)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 400); // Bad Request
    
    // Parse response
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(!body.success);
    assert!(body.error.is_some());
    assert!(body.error.unwrap().contains("Invalid IP address"));
}

#[tokio::test]
async fn test_post_record_validation_error_missing_cname_value() {
    let (_records_arc, routes) = create_test_server().await;

    // Create CNAME record request without value
    let create_request = CreateRecordRequest {
        name: "www.example.com".to_string(),
        ip: None,
        ttl: None,
        record_type: Some("CNAME".to_string()),
        class: None,
        value: None, // Missing required value for CNAME
    };

    // Make POST request
    let response = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&create_request)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 400); // Bad Request
    
    // Parse response
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(!body.success);
    assert!(body.error.is_some());
    assert!(body.error.unwrap().contains("Missing required field"));
}

#[tokio::test]
async fn test_post_record_duplicate_error() {
    let (records_arc, routes) = create_test_server().await;

    // Create first record
    let create_request = CreateRecordRequest {
        name: "duplicate.example.com".to_string(),
        ip: Some("192.168.1.1".to_string()),
        ttl: None,
        record_type: Some("A".to_string()),
        class: None,
        value: None,
    };

    // Make first POST request
    let response1 = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&create_request)
        .reply(&routes)
        .await;

    assert_eq!(response1.status(), 201); // Should succeed

    // Try to create duplicate record (same name and type)
    let duplicate_request = CreateRecordRequest {
        name: "duplicate.example.com".to_string(),
        ip: Some("10.0.0.1".to_string()), // Different IP but same name/type
        ttl: None,
        record_type: Some("A".to_string()),
        class: None,
        value: None,
    };

    // Make second POST request
    let response2 = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&duplicate_request)
        .reply(&routes)
        .await;

    assert_eq!(response2.status(), 409); // Conflict
    
    // Parse response
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response2.body()).unwrap();
    assert!(!body.success);
    assert!(body.error.is_some());
    assert!(body.error.unwrap().contains("already exists"));
    
    // Verify only one record exists
    let records = records_arc.read().await;
    assert_eq!(records.len(), 1);
}

#[tokio::test]
async fn test_post_record_empty_name_validation() {
    let (_records_arc, routes) = create_test_server().await;

    // Create record request with empty name
    let create_request = CreateRecordRequest {
        name: "".to_string(), // Empty name should fail validation
        ip: Some("192.168.1.1".to_string()),
        ttl: None,
        record_type: None,
        class: None,
        value: None,
    };

    // Make POST request
    let response = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&create_request)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 400); // Bad Request
    
    // Parse response
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(!body.success);
    assert!(body.error.is_some());
    assert!(body.error.unwrap().contains("Missing required field"));
}