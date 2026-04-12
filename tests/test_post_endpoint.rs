use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::sync::RwLock;
use warp::Filter;

use rind::update::{
    ApiResponse, CreateRecordRequest, DatastoreProvider, DnsRecord, DnsRecords,
    JsonlFileDatastoreProvider, RecordData,
};

async fn create_test_server() -> (
    Arc<RwLock<DnsRecords>>,
    impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone,
) {
    let records_arc = Arc::new(RwLock::new(DnsRecords::new()));
    let tmp = NamedTempFile::new().unwrap();
    let datastore: Arc<dyn DatastoreProvider> = Arc::new(JsonlFileDatastoreProvider::new(
        tmp.path().to_str().unwrap().to_string(),
    ));
    // Leak the tempfile guard so it survives for the test duration.
    std::mem::forget(tmp);

    let records_filter = warp::any().map({
        let records = Arc::clone(&records_arc);
        move || Arc::clone(&records)
    });
    let datastore_filter = warp::any().map({
        let ds = Arc::clone(&datastore);
        move || Arc::clone(&ds)
    });

    async fn create_record_handler(
        create_request: CreateRecordRequest,
        records: Arc<RwLock<DnsRecords>>,
        datastore: Arc<dyn DatastoreProvider>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        match rind::update::create_record_from_request(records, datastore, create_request, None)
            .await
        {
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
                    _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                };
                Ok(warp::reply::with_status(
                    warp::reply::json(&response),
                    status_code,
                ))
            }
        }
    }

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
                    _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                };
                Ok(warp::reply::with_status(
                    warp::reply::json(&response),
                    status_code,
                ))
            }
        }
    }

    let create_record_route = warp::path("records")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(records_filter.clone())
        .and(datastore_filter.clone())
        .and_then(create_record_handler);

    let get_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::get())
        .and(records_filter.clone())
        .and_then(get_record_handler);

    let routes = create_record_route.or(get_record_route).boxed();

    (records_arc, routes)
}

#[tokio::test]
async fn test_post_a_record_success() {
    let (records_arc, routes) = create_test_server().await;

    let body = serde_json::json!({
        "name": "test.example.com",
        "ttl": 600,
        "class": "IN",
        "type": "A",
        "ip": "192.168.1.1"
    });

    let response = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&body)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 201);

    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(body.success);
    let created = body.data.unwrap();
    assert_eq!(created.name, "test.example.com");
    assert_eq!(created.ttl, 600);
    assert_eq!(created.class, "IN");
    assert_eq!(
        created.data,
        RecordData::A {
            ip: "192.168.1.1".parse().unwrap()
        }
    );

    let records = records_arc.read().await;
    assert_eq!(records.len(), 1);
}

#[tokio::test]
async fn test_post_aaaa_record_success() {
    let (_records_arc, routes) = create_test_server().await;

    let body = serde_json::json!({
        "name": "v6.example.com",
        "type": "AAAA",
        "ip": "2001:db8::1"
    });

    let response = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&body)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 201);
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    let created = body.data.unwrap();
    assert_eq!(created.ttl, 300); // default
    assert_eq!(created.class, "IN"); // default
    assert_eq!(created.data.type_name(), "AAAA");
}

#[tokio::test]
async fn test_post_record_invalid_ip_rejected_at_parse() {
    let (_records_arc, routes) = create_test_server().await;

    // Malformed IP — serde rejects this at the Ipv4Addr parse step before
    // we even reach the handler, so warp returns 400 from body::json().
    let body = serde_json::json!({
        "name": "test.example.com",
        "type": "A",
        "ip": "not-an-ip"
    });

    let response = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&body)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_post_record_duplicate_error() {
    let (records_arc, routes) = create_test_server().await;

    let body = serde_json::json!({
        "name": "duplicate.example.com",
        "type": "A",
        "ip": "192.168.1.1"
    });

    let r1 = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&body)
        .reply(&routes)
        .await;
    assert_eq!(r1.status(), 201);

    let duplicate = serde_json::json!({
        "name": "duplicate.example.com",
        "type": "A",
        "ip": "10.0.0.1"
    });
    let r2 = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&duplicate)
        .reply(&routes)
        .await;
    assert_eq!(r2.status(), 409);

    let body: ApiResponse<DnsRecord> = serde_json::from_slice(r2.body()).unwrap();
    assert!(body.error.unwrap().contains("already exists"));

    assert_eq!(records_arc.read().await.len(), 1);
}

#[tokio::test]
async fn test_post_record_empty_name_validation() {
    let (_records_arc, routes) = create_test_server().await;

    let body = serde_json::json!({
        "name": "",
        "type": "A",
        "ip": "192.168.1.1"
    });

    let response = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&body)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 400);
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(body.error.unwrap().contains("Missing required field"));
}

#[tokio::test]
async fn test_post_record_missing_type_is_rejected() {
    let (_records_arc, routes) = create_test_server().await;

    // No "type" field — internally-tagged enum can't deserialize, 400.
    let body = serde_json::json!({
        "name": "no-type.example.com",
        "ip": "1.2.3.4"
    });

    let response = warp::test::request()
        .method("POST")
        .path("/records")
        .json(&body)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 400);
}
