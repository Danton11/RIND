use std::net::Ipv4Addr;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::sync::RwLock;
use warp::Filter;

use rind::update::{
    ApiResponse, DatastoreProvider, DnsRecord, DnsRecords, JsonlFileDatastoreProvider, RecordData,
    UpdateRecordRequest,
};

async fn create_test_server() -> (
    Arc<RwLock<DnsRecords>>,
    impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone,
) {
    let mut records = DnsRecords::new();
    let test_record = DnsRecord::new(
        "test.example.com".to_string(),
        300,
        "IN".to_string(),
        RecordData::A {
            ip: Ipv4Addr::new(192, 168, 1, 1),
        },
    );
    records.insert(test_record.id.clone(), test_record);
    let records_arc = Arc::new(RwLock::new(records));

    let tmp = NamedTempFile::new().unwrap();
    let datastore: Arc<dyn DatastoreProvider> = Arc::new(JsonlFileDatastoreProvider::new(
        tmp.path().to_str().unwrap().to_string(),
    ));
    std::mem::forget(tmp);

    let records_filter = warp::any().map({
        let records = Arc::clone(&records_arc);
        move || Arc::clone(&records)
    });
    let datastore_filter = warp::any().map({
        let ds = Arc::clone(&datastore);
        move || Arc::clone(&ds)
    });

    async fn update_record_handler(
        id: String,
        update_request: UpdateRecordRequest,
        records: Arc<RwLock<DnsRecords>>,
        datastore: Arc<dyn DatastoreProvider>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        match rind::update::update_record(records, datastore, &id, update_request, None).await {
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
                    _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                };
                Ok(warp::reply::with_status(
                    warp::reply::json(&response),
                    status_code,
                ))
            }
        }
    }

    let update_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::put())
        .and(warp::body::json())
        .and(records_filter.clone())
        .and(datastore_filter.clone())
        .and_then(update_record_handler);

    let routes = update_record_route.boxed();

    (records_arc, routes)
}

#[tokio::test]
async fn test_put_record_change_payload_to_aaaa() {
    let (records_arc, routes) = create_test_server().await;

    let test_id = {
        let records = records_arc.read().await;
        records.keys().next().unwrap().clone()
    };

    // Swap the A record for an AAAA by replacing `data` wholesale.
    let update_body = serde_json::json!({
        "name": "updated.example.com",
        "ttl": 600,
        "data": {
            "type": "AAAA",
            "ip": "2001:db8::1"
        }
    });

    let response = warp::test::request()
        .method("PUT")
        .path(&format!("/records/{}", test_id))
        .json(&update_body)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);

    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    let updated = body.data.unwrap();
    assert_eq!(updated.name, "updated.example.com");
    assert_eq!(updated.ttl, 600);
    assert_eq!(updated.data.type_name(), "AAAA");
}

#[tokio::test]
async fn test_put_record_not_found() {
    let (_records_arc, routes) = create_test_server().await;

    let body = serde_json::json!({ "name": "updated.example.com" });

    let response = warp::test::request()
        .method("PUT")
        .path("/records/non-existent-id")
        .json(&body)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 404);
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    assert!(body.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_put_record_partial_update_ttl_only() {
    let (records_arc, routes) = create_test_server().await;

    let (test_id, original_name, original_data) = {
        let records = records_arc.read().await;
        let record = records.values().next().unwrap();
        (record.id.clone(), record.name.clone(), record.data.clone())
    };

    // Only TTL changes — no `data` field, so the A record payload is preserved.
    let update_body = serde_json::json!({ "ttl": 900 });

    let response = warp::test::request()
        .method("PUT")
        .path(&format!("/records/{}", test_id))
        .json(&update_body)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
    let body: ApiResponse<DnsRecord> = serde_json::from_slice(response.body()).unwrap();
    let updated = body.data.unwrap();
    assert_eq!(updated.name, original_name);
    assert_eq!(updated.data, original_data);
    assert_eq!(updated.ttl, 900);
}
