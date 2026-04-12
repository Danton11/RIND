//! Single-entrypoint wiring for a RIND instance.
//!
//! `build_instance` binds the DNS + REST sockets, opens the LMDB env,
//! spawns the dispatch tasks, and hands back an `Instance` with the
//! real bound addresses. Same wiring for production (main.rs) and
//! in-process tests — no duplicate setup code and no drift.

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info};
use warp::Filter;

use crate::metrics::{self, MetricsRegistry};
use crate::server;
use crate::storage::{LmdbStore, StorageError};
use crate::update::{self, DatastoreError, DatastoreProvider, DnsRecords, LmdbDatastoreProvider};

/// Inputs needed to stand up an instance. `127.0.0.1:0` for dns_bind/api_bind
/// picks an ephemeral port; the real addr is reported back on `Instance`.
pub struct InstanceConfig {
    pub dns_bind: SocketAddr,
    pub api_bind: SocketAddr,
    pub lmdb_path: PathBuf,
    pub server_id: String,
    /// When `Some`, starts the Prometheus scrape endpoint. Tests leave this
    /// `None` so multiple instances can run in-process without port clashes.
    pub metrics_bind: Option<SocketAddr>,
}

/// A running instance. Tasks keep running until the handles are awaited or
/// aborted. Tests wrap this in a harness that aborts on drop; main.rs awaits
/// `dns_task` to block forever.
pub struct Instance {
    pub dns_addr: SocketAddr,
    pub api_addr: SocketAddr,
    pub metrics_registry: Arc<RwLock<MetricsRegistry>>,
    pub records: Arc<RwLock<DnsRecords>>,
    pub datastore: Arc<dyn DatastoreProvider>,
    pub dns_task: JoinHandle<()>,
    pub api_task: JoinHandle<()>,
}

#[derive(Debug, Error)]
pub enum InstanceError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
    #[error("datastore: {0}")]
    Datastore(#[from] DatastoreError),
    #[error("metrics: {0}")]
    Metrics(#[from] prometheus::Error),
}

pub async fn build_instance(cfg: InstanceConfig) -> Result<Instance, InstanceError> {
    std::fs::create_dir_all(&cfg.lmdb_path)?;
    let lmdb_store = Arc::new(LmdbStore::open(&cfg.lmdb_path)?);
    info!("LMDB env opened at {}", cfg.lmdb_path.display());

    let metrics_registry = Arc::new(RwLock::new(MetricsRegistry::new()?));

    let datastore: Arc<dyn DatastoreProvider> =
        Arc::new(LmdbDatastoreProvider::new(Arc::clone(&lmdb_store)));
    datastore.initialize().await?;

    let initial_records = datastore.load_all_records().await.unwrap_or_else(|e| {
        error!("Failed to load records from datastore: {}", e);
        DnsRecords::new()
    });
    let records = Arc::new(RwLock::new(initial_records));

    // Seed active_records gauge so dashboards don't start at zero on restart.
    {
        let records_guard = records.read().await;
        let metrics_guard = metrics_registry.read().await;
        metrics_guard
            .dns_metrics()
            .set_active_records_count(records_guard.len() as f64);
        info!("Initialized active records count: {}", records_guard.len());
    }

    // Bind the DNS socket up front so we can report the real port (ephemeral
    // :0 binds don't resolve until local_addr() is called).
    let dns_socket = Arc::new(UdpSocket::bind(cfg.dns_bind).await?);
    let dns_addr = dns_socket.local_addr()?;
    std::env::set_var("SERVER_ID", &cfg.server_id);

    // Build REST routes. Handlers live in this module — they're the thin
    // glue between warp filters and the async fns in `update`.
    let api_routes = build_api_routes(
        Arc::clone(&records),
        Arc::clone(&datastore),
        Arc::clone(&metrics_registry),
    );

    let (api_addr, api_fut) = warp::serve(api_routes).bind_ephemeral(cfg.api_bind);
    let api_task = tokio::spawn(api_fut);
    info!("API server listening on {}", api_addr);

    // Optional Prometheus endpoint — skipped in tests.
    if let Some(addr) = cfg.metrics_bind {
        let server = metrics::MetricsServer::new(Arc::clone(&metrics_registry));
        tokio::spawn(async move {
            if let Err(e) = server.start(addr).await {
                error!("Metrics server failed to start: {}", e);
            }
        });
        info!("Metrics server listening on http://{}/metrics", addr);
    }

    let records_for_server = Arc::clone(&records);
    let metrics_for_server = Arc::clone(&metrics_registry);
    let dns_socket_for_server = Arc::clone(&dns_socket);
    let dns_task = tokio::spawn(async move {
        if let Err(e) = server::run(
            dns_socket_for_server,
            records_for_server,
            metrics_for_server,
        )
        .await
        {
            error!("DNS server error: {}", e);
        }
    });
    info!("DNS server listening on {}", dns_addr);

    Ok(Instance {
        dns_addr,
        api_addr,
        metrics_registry,
        records,
        datastore,
        dns_task,
        api_task,
    })
}

// ---------- REST handler glue ----------------------------------------------
//
// These are 1:1 with the functions in `update`, wrapping them in warp-shaped
// error mapping + metrics bookkeeping. Kept private to this module.

fn build_api_routes(
    records: Arc<RwLock<DnsRecords>>,
    datastore: Arc<dyn DatastoreProvider>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
) -> warp::filters::BoxedFilter<(Box<dyn warp::Reply>,)> {
    let records_filter = {
        let r = Arc::clone(&records);
        warp::any().map(move || Arc::clone(&r))
    };
    let datastore_filter = {
        let d = Arc::clone(&datastore);
        warp::any().map(move || Arc::clone(&d))
    };
    let metrics_filter = {
        let m = Arc::clone(&metrics_registry);
        warp::any().map(move || Arc::clone(&m))
    };

    let get_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::get())
        .and(records_filter.clone())
        .and(metrics_filter.clone())
        .and_then(get_record_handler);

    let update_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::put())
        .and(warp::body::json())
        .and(records_filter.clone())
        .and(datastore_filter.clone())
        .and(metrics_filter.clone())
        .and_then(update_record_handler);

    let delete_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::delete())
        .and(records_filter.clone())
        .and(datastore_filter.clone())
        .and(metrics_filter.clone())
        .and_then(delete_record_handler);

    let list_records_route = warp::path("records")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .and(records_filter.clone())
        .and(metrics_filter.clone())
        .and_then(list_records_handler);

    let create_record_route = warp::path("records")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(records_filter)
        .and(datastore_filter)
        .and(metrics_filter)
        .and_then(create_record_handler);

    // `.boxed()` collapses the nested filter type into a `BoxedFilter`,
    // which sidesteps a variance/lifetime inference problem when handler
    // args include `Arc<dyn Trait>`. All branches return `Box<dyn Reply>`
    // for a uniform filter output type.
    get_record_route
        .or(update_record_route)
        .unify()
        .or(delete_record_route)
        .unify()
        .or(list_records_route)
        .unify()
        .or(create_record_route)
        .unify()
        .boxed()
}

async fn record_api_metrics(
    endpoint: &str,
    method: &str,
    status_code: u16,
    duration: f64,
    metrics_registry: &Arc<RwLock<MetricsRegistry>>,
) {
    let registry = metrics_registry.read().await;
    let status_str = status_code.to_string();
    registry
        .dns_metrics()
        .record_api_request(endpoint, method, &status_str, duration);

    if status_code >= 400 {
        let error_type = match status_code {
            400 => "bad_request",
            401 => "unauthorized",
            403 => "forbidden",
            404 => "not_found",
            409 => "conflict",
            500 => "internal_server_error",
            _ => "other_error",
        };
        registry
            .dns_metrics()
            .record_api_error(endpoint, error_type);
    }
}

fn status_for(code: u16) -> warp::http::StatusCode {
    match code {
        404 => warp::http::StatusCode::NOT_FOUND,
        400 => warp::http::StatusCode::BAD_REQUEST,
        409 => warp::http::StatusCode::CONFLICT,
        500 => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn get_record_handler(
    id: String,
    records: Arc<RwLock<DnsRecords>>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
    let start = std::time::Instant::now();
    let (endpoint, method) = ("/records/{id}", "GET");

    let reply: Box<dyn warp::Reply> =
        match update::get_record(records, &id, Some(metrics_registry.clone())).await {
            Ok(record) => {
                record_api_metrics(
                    endpoint,
                    method,
                    200,
                    start.elapsed().as_secs_f64(),
                    &metrics_registry,
                )
                .await;
                Box::new(warp::reply::with_status(
                    warp::reply::json(&update::ApiResponse::success(record)),
                    warp::http::StatusCode::OK,
                ))
            }
            Err(e) => {
                let code = status_for(e.to_status_code());
                record_api_metrics(
                    endpoint,
                    method,
                    code.as_u16(),
                    start.elapsed().as_secs_f64(),
                    &metrics_registry,
                )
                .await;
                Box::new(warp::reply::with_status(
                    warp::reply::json(&update::ApiResponse::<update::DnsRecord>::error(
                        e.to_string(),
                    )),
                    code,
                ))
            }
        };
    Ok(reply)
}

async fn update_record_handler(
    id: String,
    update_request: update::UpdateRecordRequest,
    records: Arc<RwLock<DnsRecords>>,
    datastore: Arc<dyn DatastoreProvider>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
    let start = std::time::Instant::now();
    let (endpoint, method) = ("/records/{id}", "PUT");

    let reply: Box<dyn warp::Reply> = match update::update_record(
        records,
        datastore,
        &id,
        update_request,
        Some(metrics_registry.clone()),
    )
    .await
    {
        Ok(record) => {
            record_api_metrics(
                endpoint,
                method,
                200,
                start.elapsed().as_secs_f64(),
                &metrics_registry,
            )
            .await;
            Box::new(warp::reply::with_status(
                warp::reply::json(&update::ApiResponse::success(record)),
                warp::http::StatusCode::OK,
            ))
        }
        Err(e) => {
            let code = status_for(e.to_status_code());
            record_api_metrics(
                endpoint,
                method,
                code.as_u16(),
                start.elapsed().as_secs_f64(),
                &metrics_registry,
            )
            .await;
            Box::new(warp::reply::with_status(
                warp::reply::json(&update::ApiResponse::<update::DnsRecord>::error(
                    e.to_string(),
                )),
                code,
            ))
        }
    };
    Ok(reply)
}

async fn delete_record_handler(
    id: String,
    records: Arc<RwLock<DnsRecords>>,
    datastore: Arc<dyn DatastoreProvider>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
    let start = std::time::Instant::now();
    let (endpoint, method) = ("/records/{id}", "DELETE");

    let reply: Box<dyn warp::Reply> = match update::delete_record(
        records,
        datastore,
        &id,
        Some(metrics_registry.clone()),
    )
    .await
    {
        Ok(()) => {
            record_api_metrics(
                endpoint,
                method,
                204,
                start.elapsed().as_secs_f64(),
                &metrics_registry,
            )
            .await;
            Box::new(warp::reply::with_status(
                warp::reply::json(&update::ApiResponse::<()>::success(())),
                warp::http::StatusCode::NO_CONTENT,
            ))
        }
        Err(e) => {
            let code = status_for(e.to_status_code());
            record_api_metrics(
                endpoint,
                method,
                code.as_u16(),
                start.elapsed().as_secs_f64(),
                &metrics_registry,
            )
            .await;
            Box::new(warp::reply::with_status(
                warp::reply::json(&update::ApiResponse::<()>::error(e.to_string())),
                code,
            ))
        }
    };
    Ok(reply)
}

async fn list_records_handler(
    query_params: std::collections::HashMap<String, String>,
    records: Arc<RwLock<DnsRecords>>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
    let start = std::time::Instant::now();
    let (endpoint, method) = ("/records", "GET");

    let page = query_params
        .get("page")
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(1);
    let per_page = query_params
        .get("per_page")
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(50);

    let reply: Box<dyn warp::Reply> =
        match update::list_records(records, page, per_page, Some(metrics_registry.clone())).await {
            Ok(list) => {
                record_api_metrics(
                    endpoint,
                    method,
                    200,
                    start.elapsed().as_secs_f64(),
                    &metrics_registry,
                )
                .await;
                Box::new(warp::reply::with_status(
                    warp::reply::json(&update::ApiResponse::success(list)),
                    warp::http::StatusCode::OK,
                ))
            }
            Err(e) => {
                let code = status_for(e.to_status_code());
                record_api_metrics(
                    endpoint,
                    method,
                    code.as_u16(),
                    start.elapsed().as_secs_f64(),
                    &metrics_registry,
                )
                .await;
                Box::new(warp::reply::with_status(
                    warp::reply::json(&update::ApiResponse::<update::RecordListResponse>::error(
                        e.to_string(),
                    )),
                    code,
                ))
            }
        };
    Ok(reply)
}

async fn create_record_handler(
    create_request: update::CreateRecordRequest,
    records: Arc<RwLock<DnsRecords>>,
    datastore: Arc<dyn DatastoreProvider>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
    let start = std::time::Instant::now();
    let (endpoint, method) = ("/records", "POST");

    let reply: Box<dyn warp::Reply> = match update::create_record_from_request(
        records,
        datastore,
        create_request,
        Some(metrics_registry.clone()),
    )
    .await
    {
        Ok(record) => {
            record_api_metrics(
                endpoint,
                method,
                201,
                start.elapsed().as_secs_f64(),
                &metrics_registry,
            )
            .await;
            Box::new(warp::reply::with_status(
                warp::reply::json(&update::ApiResponse::success(record)),
                warp::http::StatusCode::CREATED,
            ))
        }
        Err(e) => {
            let code = status_for(e.to_status_code());
            record_api_metrics(
                endpoint,
                method,
                code.as_u16(),
                start.elapsed().as_secs_f64(),
                &metrics_registry,
            )
            .await;
            Box::new(warp::reply::with_status(
                warp::reply::json(&update::ApiResponse::<update::DnsRecord>::error(
                    e.to_string(),
                )),
                code,
            ))
        }
    };
    Ok(reply)
}
