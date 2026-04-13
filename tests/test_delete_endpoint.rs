use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

use rind::update::{ApiResponse, DatastoreProvider, DnsRecord, DnsRecords, RecordData};

mod common;
use common::InMemoryDatastoreProvider;

fn a_record(name: &str, ip: [u8; 4]) -> DnsRecord {
    DnsRecord::new(
        name.to_string(),
        300,
        "IN".to_string(),
        RecordData::A {
            ip: Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]),
        },
    )
}

async fn create_test_server() -> (
    Arc<RwLock<DnsRecords>>,
    impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone,
) {
    let mut records = DnsRecords::new();
    let test_record = a_record("test.example.com", [192, 168, 1, 1]);
    records.insert(test_record.id.clone(), test_record);

    let datastore: Arc<dyn DatastoreProvider> =
        Arc::new(InMemoryDatastoreProvider::with_records(records.clone()));
    let records_arc = Arc::new(RwLock::new(records));

    let records_filter = warp::any().map({
        let records = Arc::clone(&records_arc);
        move || Arc::clone(&records)
    });
    let datastore_filter = warp::any().map({
        let ds = Arc::clone(&datastore);
        move || Arc::clone(&ds)
    });

    async fn delete_record_handler(
        id: String,
        records: Arc<RwLock<DnsRecords>>,
        datastore: Arc<dyn DatastoreProvider>,
    ) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
        match rind::update::delete_record(records, datastore, &id, None).await {
            Ok(()) => Ok(Box::new(warp::reply::with_status(
                warp::reply(),
                warp::http::StatusCode::NO_CONTENT,
            ))),
            Err(e) => {
                let response = ApiResponse::<()>::error(e.to_string());
                let status_code = match e.to_status_code() {
                    404 => warp::http::StatusCode::NOT_FOUND,
                    400 => warp::http::StatusCode::BAD_REQUEST,
                    409 => warp::http::StatusCode::CONFLICT,
                    _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                };
                Ok(Box::new(warp::reply::with_status(
                    warp::reply::json(&response),
                    status_code,
                )))
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

    let delete_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::delete())
        .and(records_filter.clone())
        .and(datastore_filter.clone())
        .and_then(delete_record_handler);

    let get_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::get())
        .and(records_filter.clone())
        .and_then(get_record_handler);

    let routes = delete_record_route.or(get_record_route).boxed();

    (records_arc, routes)
}

#[tokio::test]
async fn test_delete_record_success() {
    let (records_arc, routes) = create_test_server().await;

    let test_id = {
        let records = records_arc.read().await;
        records.keys().next().unwrap().clone()
    };

    let response = warp::test::request()
        .method("GET")
        .path(&format!("/records/{}", test_id))
        .reply(&routes)
        .await;
    assert_eq!(response.status(), 200);

    let response = warp::test::request()
        .method("DELETE")
        .path(&format!("/records/{}", test_id))
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 204);
    assert_eq!(response.body().len(), 0);

    let response = warp::test::request()
        .method("GET")
        .path(&format!("/records/{}", test_id))
        .reply(&routes)
        .await;
    assert_eq!(response.status(), 404);

    assert_eq!(records_arc.read().await.len(), 0);
}

#[tokio::test]
async fn test_delete_record_not_found() {
    let (_records_arc, routes) = create_test_server().await;

    let response = warp::test::request()
        .method("DELETE")
        .path("/records/non-existent-id")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 404);
    let body: ApiResponse<()> = serde_json::from_slice(response.body()).unwrap();
    assert!(!body.success);
    assert!(body.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_delete_record_multiple_records() {
    let mut records = DnsRecords::new();
    let record1 = a_record("test1.example.com", [192, 168, 1, 1]);
    let record2 = a_record("test2.example.com", [192, 168, 1, 2]);
    let record1_id = record1.id.clone();
    let record2_id = record2.id.clone();
    records.insert(record1_id.clone(), record1);
    records.insert(record2_id.clone(), record2);

    let datastore: Arc<dyn DatastoreProvider> =
        Arc::new(InMemoryDatastoreProvider::with_records(records.clone()));
    let records_arc = Arc::new(RwLock::new(records));

    let records_filter = warp::any().map({
        let records = Arc::clone(&records_arc);
        move || Arc::clone(&records)
    });
    let datastore_filter = warp::any().map({
        let ds = Arc::clone(&datastore);
        move || Arc::clone(&ds)
    });

    async fn delete_record_handler(
        id: String,
        records: Arc<RwLock<DnsRecords>>,
        datastore: Arc<dyn DatastoreProvider>,
    ) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
        match rind::update::delete_record(records, datastore, &id, None).await {
            Ok(()) => Ok(Box::new(warp::reply::with_status(
                warp::reply(),
                warp::http::StatusCode::NO_CONTENT,
            ))),
            Err(e) => {
                let response = ApiResponse::<()>::error(e.to_string());
                let status_code = match e.to_status_code() {
                    404 => warp::http::StatusCode::NOT_FOUND,
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
        .and(datastore_filter.clone())
        .and_then(delete_record_handler)
        .boxed();

    assert_eq!(records_arc.read().await.len(), 2);

    let response = warp::test::request()
        .method("DELETE")
        .path(&format!("/records/{}", record1_id))
        .reply(&delete_record_route)
        .await;
    assert_eq!(response.status(), 204);

    {
        let records = records_arc.read().await;
        assert_eq!(records.len(), 1);
        assert!(records.contains_key(&record2_id));
        assert!(!records.contains_key(&record1_id));
    }

    let response = warp::test::request()
        .method("DELETE")
        .path(&format!("/records/{}", record2_id))
        .reply(&delete_record_route)
        .await;
    assert_eq!(response.status(), 204);
    assert_eq!(records_arc.read().await.len(), 0);
}

#[tokio::test]
async fn test_delete_record_idempotent() {
    let (records_arc, routes) = create_test_server().await;

    let test_id = {
        let records = records_arc.read().await;
        records.keys().next().unwrap().clone()
    };

    let response = warp::test::request()
        .method("DELETE")
        .path(&format!("/records/{}", test_id))
        .reply(&routes)
        .await;
    assert_eq!(response.status(), 204);

    let response = warp::test::request()
        .method("DELETE")
        .path(&format!("/records/{}", test_id))
        .reply(&routes)
        .await;
    assert_eq!(response.status(), 404);

    let body: ApiResponse<()> = serde_json::from_slice(response.body()).unwrap();
    assert!(body.error.unwrap().contains("not found"));
}
