use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;
use serde_json::json;
use std::net::Ipv4Addr;

use rind::update::{DnsRecord, DnsRecords, UpdateRecordRequest, ApiResponse};

// Helper function to create a test server with some initial records
async fn create_test_server() -> (Arc<RwLock<DnsRecords>>, impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone) {
    let mut records = DnsRecords::new();
    
    // Add a test record
    let test_record = DnsRecord::new(
        "test.example.com".to_string(),
        Some(Ipv4Addr::new(192, 168, 1, 1)),
        300,
        "A".to_string(),
        "IN".to_string(),
        None,
    );
    let test_id = test_record.id.clone();
    records.insert(test_id, test_record);
    
    let records_arc = Arc::new(RwLock::new(records));
    let records_filter = warp::any().map({
        let records = Arc::clone(&records_arc);
        move || Arc::clone(&records)
    });

    // Handler for PUT /records/{id} endpoint (copied from main.rs)
    async fn update_record_handler(
        id: String,
        update_request: UpdateRecordRequest,
        records: Arc<RwLock<DnsRecords>>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        match rind::update::update_record(records, &id, update_request, None).await {
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

    // PUT /records/{id} - Update an existing record by ID
    let update_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::put())
        .and(warp::body::json())
        .and(records_filter.clone())
        .and_then(update_record_handler);

    // GET /records/{id} - Retrieve a specific record by ID
    let get_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::get())
        .and(records_filter.clone())
        .and_then(get_record_handler);

    let routes = update_record_route.or(get_record_route);
    
    (records_arc, routes)
}

#[tokio::test]
async fn test_put_record_success() {
    let (records_arc, routes) = create_test_server().await;
    
    // Get the test record ID
    let test_id = {
        let records = records_arc.read().await;
        records.keys().next().unwrap().clone()
    };

    // Create update request
    let update_request = UpdateRecordRequest {
        name: Some("updated.example.com".to_string()),
        ip: Some("10.0.0.1".to_string()),
        ttl: Some(600),
        record_type: None, // Keep existing
        class: None,       // Keep existing
        value: None,       // Keep existing
    };

    // Make PUT request
    let response = warp::test::request()
        .method("PUT")
        .path(&format!("/records/{}", test_id))
        .json(&update_request)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
    
    // Parse response
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(body.success);
    assert!(body.data.is_some());
    
    let updated_record = body.data.unwrap();
    assert_eq!(updated_record.name, "updated.example.com");
    assert_eq!(updated_record.ip, Some(Ipv4Addr::new(10, 0, 0, 1)));
    assert_eq!(updated_record.ttl, 600);
    assert_eq!(updated_record.record_type, "A"); // Should remain unchanged
    assert_eq!(updated_record.class, "IN");      // Should remain unchanged
}

#[tokio::test]
async fn test_put_record_not_found() {
    let (_records_arc, routes) = create_test_server().await;
    
    let update_request = UpdateRecordRequest {
        name: Some("updated.example.com".to_string()),
        ip: None,
        ttl: None,
        record_type: None,
        class: None,
        value: None,
    };

    // Make PUT request with non-existent ID
    let response = warp::test::request()
        .method("PUT")
        .path("/records/non-existent-id")
        .json(&update_request)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 404);
    
    // Parse response
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(!body.success);
    assert!(body.error.is_some());
    assert!(body.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_put_record_validation_error() {
    let (records_arc, routes) = create_test_server().await;
    
    // Get the test record ID
    let test_id = {
        let records = records_arc.read().await;
        records.keys().next().unwrap().clone()
    };

    // Create update request with invalid IP
    let update_request = UpdateRecordRequest {
        name: None,
        ip: Some("invalid-ip".to_string()), // Invalid IP address
        ttl: None,
        record_type: None,
        class: None,
        value: None,
    };

    // Make PUT request
    let response = warp::test::request()
        .method("PUT")
        .path(&format!("/records/{}", test_id))
        .json(&update_request)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 400);
    
    // Parse response
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(!body.success);
    assert!(body.error.is_some());
    assert!(body.error.unwrap().contains("Invalid IP address"));
}

#[tokio::test]
async fn test_put_record_partial_update() {
    let (records_arc, routes) = create_test_server().await;
    
    // Get the test record ID and original data
    let (test_id, original_name, original_ip) = {
        let records = records_arc.read().await;
        let record = records.values().next().unwrap();
        (record.id.clone(), record.name.clone(), record.ip)
    };

    // Create update request that only updates TTL
    let update_request = UpdateRecordRequest {
        name: None,    // Keep existing
        ip: None,      // Keep existing
        ttl: Some(900), // Update only TTL
        record_type: None,
        class: None,
        value: None,
    };

    // Make PUT request
    let response = warp::test::request()
        .method("PUT")
        .path(&format!("/records/{}", test_id))
        .json(&update_request)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
    
    // Parse response
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(body.success);
    
    let updated_record = body.data.unwrap();
    assert_eq!(updated_record.name, original_name); // Should remain unchanged
    assert_eq!(updated_record.ip, original_ip);     // Should remain unchanged
    assert_eq!(updated_record.ttl, 900);           // Should be updated
}