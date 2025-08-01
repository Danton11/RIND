use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;
use serde_json::json;
use std::net::Ipv4Addr;

use rind::update::{DnsRecord, DnsRecords, ApiResponse};

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

    // Handler for DELETE /records/{id} endpoint (copied from main.rs)
    async fn delete_record_handler(
        id: String,
        records: Arc<RwLock<DnsRecords>>,
    ) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
        match rind::update::delete_record(records, &id, None).await {
            Ok(()) => {
                // Return HTTP 204 No Content on successful deletion
                Ok(Box::new(warp::reply::with_status(
                    warp::reply(),
                    warp::http::StatusCode::NO_CONTENT,
                )))
            }
            Err(e) => {
                let response = ApiResponse::<()>::error(e.to_string());
                let status_code = match e.to_status_code() {
                    404 => warp::http::StatusCode::NOT_FOUND,
                    400 => warp::http::StatusCode::BAD_REQUEST,
                    409 => warp::http::StatusCode::CONFLICT,
                    500 => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                    _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                };
                Ok(Box::new(warp::reply::with_status(
                    warp::reply::json(&response),
                    status_code,
                )))
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

    // DELETE /records/{id} - Delete a record by ID
    let delete_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::delete())
        .and(records_filter.clone())
        .and_then(delete_record_handler);

    // GET /records/{id} - Retrieve a specific record by ID (for verification)
    let get_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::get())
        .and(records_filter.clone())
        .and_then(get_record_handler);

    let routes = delete_record_route.or(get_record_route);
    
    (records_arc, routes)
}

#[tokio::test]
async fn test_delete_record_success() {
    let (records_arc, routes) = create_test_server().await;
    
    // Get the test record ID
    let test_id = {
        let records = records_arc.read().await;
        records.keys().next().unwrap().clone()
    };

    // Verify record exists before deletion
    let response = warp::test::request()
        .method("GET")
        .path(&format!("/records/{}", test_id))
        .reply(&routes)
        .await;
    assert_eq!(response.status(), 200);

    // Make DELETE request
    let response = warp::test::request()
        .method("DELETE")
        .path(&format!("/records/{}", test_id))
        .reply(&routes)
        .await;

    // Should return HTTP 204 No Content
    assert_eq!(response.status(), 204);
    
    // Response body should be empty
    assert_eq!(response.body().len(), 0);

    // Verify record is actually deleted by trying to get it
    let response = warp::test::request()
        .method("GET")
        .path(&format!("/records/{}", test_id))
        .reply(&routes)
        .await;
    
    assert_eq!(response.status(), 404);
    
    // Verify the record count in memory
    let records = records_arc.read().await;
    assert_eq!(records.len(), 0);
}

#[tokio::test]
async fn test_delete_record_not_found() {
    let (_records_arc, routes) = create_test_server().await;
    
    // Make DELETE request with non-existent ID
    let response = warp::test::request()
        .method("DELETE")
        .path("/records/non-existent-id")
        .reply(&routes)
        .await;

    // Should return HTTP 404 Not Found
    assert_eq!(response.status(), 404);
    
    // Parse response
    let body: ApiResponse<()> = serde_json::from_slice(response.body()).unwrap();
    assert!(!body.success);
    assert!(body.error.is_some());
    assert!(body.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_delete_record_multiple_records() {
    let mut records = DnsRecords::new();
    
    // Add multiple test records
    let record1 = DnsRecord::new(
        "test1.example.com".to_string(),
        Some(Ipv4Addr::new(192, 168, 1, 1)),
        300,
        "A".to_string(),
        "IN".to_string(),
        None,
    );
    let record2 = DnsRecord::new(
        "test2.example.com".to_string(),
        Some(Ipv4Addr::new(192, 168, 1, 2)),
        300,
        "A".to_string(),
        "IN".to_string(),
        None,
    );
    
    let record1_id = record1.id.clone();
    let record2_id = record2.id.clone();
    
    records.insert(record1_id.clone(), record1);
    records.insert(record2_id.clone(), record2);
    
    let records_arc = Arc::new(RwLock::new(records));
    let records_filter = warp::any().map({
        let records = Arc::clone(&records_arc);
        move || Arc::clone(&records)
    });

    // Handler for DELETE /records/{id} endpoint
    async fn delete_record_handler(
        id: String,
        records: Arc<RwLock<DnsRecords>>,
    ) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
        match rind::update::delete_record(records, &id, None).await {
            Ok(()) => {
                Ok(Box::new(warp::reply::with_status(
                    warp::reply(),
                    warp::http::StatusCode::NO_CONTENT,
                )))
            }
            Err(e) => {
                let response = ApiResponse::<()>::error(e.to_string());
                let status_code = match e.to_status_code() {
                    404 => warp::http::StatusCode::NOT_FOUND,
                    400 => warp::http::StatusCode::BAD_REQUEST,
                    409 => warp::http::StatusCode::CONFLICT,
                    500 => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                    _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                };
                Ok(Box::new(warp::reply::with_status(
                    warp::reply::json(&response),
                    status_code,
                )))
            }
        }
    }

    let delete_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::delete())
        .and(records_filter.clone())
        .and_then(delete_record_handler);

    // Verify we start with 2 records
    {
        let records = records_arc.read().await;
        assert_eq!(records.len(), 2);
    }

    // Delete first record
    let response = warp::test::request()
        .method("DELETE")
        .path(&format!("/records/{}", record1_id))
        .reply(&delete_record_route)
        .await;
    
    assert_eq!(response.status(), 204);

    // Verify we now have 1 record and it's the correct one
    {
        let records = records_arc.read().await;
        assert_eq!(records.len(), 1);
        assert!(records.contains_key(&record2_id));
        assert!(!records.contains_key(&record1_id));
    }

    // Delete second record
    let response = warp::test::request()
        .method("DELETE")
        .path(&format!("/records/{}", record2_id))
        .reply(&delete_record_route)
        .await;
    
    assert_eq!(response.status(), 204);

    // Verify we now have 0 records
    {
        let records = records_arc.read().await;
        assert_eq!(records.len(), 0);
    }
}

#[tokio::test]
async fn test_delete_record_idempotent() {
    let (records_arc, routes) = create_test_server().await;
    
    // Get the test record ID
    let test_id = {
        let records = records_arc.read().await;
        records.keys().next().unwrap().clone()
    };

    // Delete the record first time
    let response = warp::test::request()
        .method("DELETE")
        .path(&format!("/records/{}", test_id))
        .reply(&routes)
        .await;
    
    assert_eq!(response.status(), 204);

    // Try to delete the same record again
    let response = warp::test::request()
        .method("DELETE")
        .path(&format!("/records/{}", test_id))
        .reply(&routes)
        .await;
    
    // Should return 404 since record no longer exists
    assert_eq!(response.status(), 404);
    
    let body: ApiResponse<()> = serde_json::from_slice(response.body()).unwrap();
    assert!(!body.success);
    assert!(body.error.is_some());
    assert!(body.error.unwrap().contains("not found"));
}