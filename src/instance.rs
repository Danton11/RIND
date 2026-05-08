//! Single-entrypoint wiring for a RIND instance.
//!
//! `build_instance` binds the DNS + REST sockets, opens the LMDB env,
//! spawns the dispatch tasks, and hands back an `Instance` with the
//! real bound addresses. Same wiring for production (main.rs) and
//! in-process tests — no duplicate setup code and no drift.

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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
use crate::update;

/// Operating mode for the RIND instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RindMode {
    /// LMDB is the authoritative store. REST writes go directly to LMDB.
    Standalone,
    /// etcd (via K8s CRD) is the authoritative store. LMDB is a local cache
    /// populated by the CRD watcher. REST writes proxy to the K8s API.
    #[cfg(feature = "kubernetes")]
    Kubernetes { namespace: String },
}

impl RindMode {
    pub fn from_env() -> Self {
        match std::env::var("RIND_MODE").as_deref() {
            #[cfg(feature = "kubernetes")]
            Ok("kubernetes") => {
                let namespace =
                    std::env::var("RIND_NAMESPACE").unwrap_or_else(|_| "rind-system".to_string());
                RindMode::Kubernetes { namespace }
            }
            _ => RindMode::Standalone,
        }
    }
}

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
    /// Operating mode. Defaults to `Standalone` for backward compatibility.
    pub mode: RindMode,
}

/// A running instance. Tasks keep running until the handles are awaited or
/// aborted. Tests wrap this in a harness that aborts on drop; main.rs awaits
/// `dns_task` to block forever.
pub struct Instance {
    pub dns_addr: SocketAddr,
    pub api_addr: SocketAddr,
    pub metrics_registry: Arc<RwLock<MetricsRegistry>>,
    pub store: Arc<LmdbStore>,
    pub dns_task: JoinHandle<()>,
    pub api_task: JoinHandle<()>,
    /// Readiness gate read by `/health`. Standalone instances start ready;
    /// kubernetes-mode instances flip this to `true` after the watcher's
    /// initial sync. Exposed so tests can flip it directly.
    pub ready: Arc<AtomicBool>,
}

#[derive(Debug, Error)]
pub enum InstanceError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
    #[error("metrics: {0}")]
    Metrics(#[from] prometheus::Error),
}

pub async fn build_instance(cfg: InstanceConfig) -> Result<Instance, InstanceError> {
    std::fs::create_dir_all(&cfg.lmdb_path)?;
    let store = Arc::new(LmdbStore::open(&cfg.lmdb_path)?);
    info!("LMDB env opened at {}", cfg.lmdb_path.display());

    let metrics_registry = Arc::new(RwLock::new(MetricsRegistry::new()?));

    // /health gates traffic on initial readiness. Standalone is ready as soon
    // as LMDB is open; kubernetes mode flips this true after the watcher's
    // first full sync, so fresh pods don't serve NXDOMAIN before the cache is
    // populated.
    let ready = Arc::new(AtomicBool::new(matches!(cfg.mode, RindMode::Standalone)));

    // Seed active_records gauge so dashboards don't start at zero on restart.
    {
        let count = store.record_count()?;
        let metrics_guard = metrics_registry.read().await;
        metrics_guard
            .dns_metrics()
            .set_active_records_count(count as f64);
        info!("Initialized active records count: {}", count);
    }

    // Bind the DNS socket up front so we can report the real port (ephemeral
    // :0 binds don't resolve until local_addr() is called).
    let dns_socket = Arc::new(UdpSocket::bind(cfg.dns_bind).await?);
    let dns_addr = dns_socket.local_addr()?;
    std::env::set_var("SERVER_ID", &cfg.server_id);

    // In kubernetes mode, build a single kube::Client up front and share it
    // with both the REST write shim and the CRD watcher (cloning is cheap —
    // the client is `Arc` internally).
    #[cfg(feature = "kubernetes")]
    let kube_client = if matches!(cfg.mode, RindMode::Kubernetes { .. }) {
        Some(
            kube::Client::try_default()
                .await
                .expect("Failed to create kube client — is KUBECONFIG set or running in-cluster?"),
        )
    } else {
        None
    };

    // Build REST routes. Handlers live in this module — they're the thin
    // glue between warp filters and the async fns in `update`.
    #[cfg(feature = "kubernetes")]
    let write_backend = if let RindMode::Kubernetes { ref namespace } = cfg.mode {
        WriteBackend::Kubernetes(Arc::new(crate::crd::KubeWriteClient::new(
            kube_client
                .clone()
                .expect("kube_client must exist in Kubernetes mode"),
            namespace,
            Arc::clone(&store),
        )))
    } else {
        WriteBackend::Local
    };
    #[cfg(not(feature = "kubernetes"))]
    let write_backend = WriteBackend::Local;

    let api_routes = build_api_routes(
        Arc::clone(&store),
        Arc::clone(&metrics_registry),
        write_backend,
        Arc::clone(&ready),
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

    // In kubernetes mode, spawn the CRD watcher that syncs records to LMDB.
    #[cfg(feature = "kubernetes")]
    if let RindMode::Kubernetes { ref namespace } = cfg.mode {
        let watcher = crate::watcher::CrdWatcher::new(
            Arc::clone(&store),
            kube_client
                .clone()
                .expect("kube_client must exist in Kubernetes mode"),
            namespace.clone(),
            Arc::clone(&metrics_registry),
            Arc::clone(&ready),
        )
        .expect("Failed to create CRD watcher");

        watcher
            .register_metrics()
            .await
            .expect("Failed to register watcher metrics");

        tokio::spawn(async move {
            if let Err(e) = watcher.run().await {
                error!(
                    "CRD watcher exited, killing process so kubelet restarts the pod: {}",
                    e
                );
                std::process::exit(1);
            }
        });
        info!(namespace = %namespace, "CRD watcher spawned");
    }

    let store_for_server = Arc::clone(&store);
    let metrics_for_server = Arc::clone(&metrics_registry);
    let dns_socket_for_server = Arc::clone(&dns_socket);
    let dns_task = tokio::spawn(async move {
        if let Err(e) =
            server::run(dns_socket_for_server, store_for_server, metrics_for_server).await
        {
            error!("DNS server error: {}", e);
        }
    });
    info!("DNS server listening on {}", dns_addr);

    Ok(Instance {
        dns_addr,
        api_addr,
        metrics_registry,
        store,
        dns_task,
        api_task,
        ready,
    })
}

// ---------- REST handler glue ----------------------------------------------
//
// These are 1:1 with the functions in `update`, wrapping them in warp-shaped
// error mapping + metrics bookkeeping. Kept private to this module.

/// Write backend passed to mutation handlers. In standalone mode, writes go
/// directly to LMDB. In kubernetes mode, writes proxy to the K8s API.
#[derive(Clone)]
enum WriteBackend {
    Local,
    #[cfg(feature = "kubernetes")]
    Kubernetes(Arc<crate::crd::KubeWriteClient>),
}

fn build_api_routes(
    store: Arc<LmdbStore>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
    write_backend: WriteBackend,
    ready: Arc<AtomicBool>,
) -> warp::filters::BoxedFilter<(Box<dyn warp::Reply>,)> {
    let store_filter = {
        let s = Arc::clone(&store);
        warp::any().map(move || Arc::clone(&s))
    };
    let metrics_filter = {
        let m = Arc::clone(&metrics_registry);
        warp::any().map(move || Arc::clone(&m))
    };
    let backend_filter = {
        let b = write_backend;
        warp::any().map(move || b.clone())
    };

    let health_route = {
        let r = Arc::clone(&ready);
        warp::path("health")
            .and(warp::path::end())
            .and(warp::get())
            .map(move || {
                let body = warp::reply::json(&serde_json::json!({
                    "ready": r.load(Ordering::Relaxed),
                }));
                let status = if r.load(Ordering::Relaxed) {
                    warp::http::StatusCode::OK
                } else {
                    warp::http::StatusCode::SERVICE_UNAVAILABLE
                };
                Box::new(warp::reply::with_status(body, status)) as Box<dyn warp::Reply>
            })
    };

    let get_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::get())
        .and(store_filter.clone())
        .and(metrics_filter.clone())
        .and_then(get_record_handler);

    let update_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::put())
        .and(warp::body::json())
        .and(store_filter.clone())
        .and(metrics_filter.clone())
        .and(backend_filter.clone())
        .and_then(update_record_handler);

    let delete_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::delete())
        .and(store_filter.clone())
        .and(metrics_filter.clone())
        .and(backend_filter.clone())
        .and_then(delete_record_handler);

    let list_records_route = warp::path("records")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .and(store_filter.clone())
        .and(metrics_filter.clone())
        .and_then(list_records_handler);

    let create_record_route = warp::path("records")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(store_filter)
        .and(metrics_filter)
        .and(backend_filter)
        .and_then(create_record_handler);

    health_route
        .or(get_record_route)
        .unify()
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
    store: Arc<LmdbStore>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
    let start = std::time::Instant::now();
    let (endpoint, method) = ("/records/{id}", "GET");

    let reply: Box<dyn warp::Reply> =
        match update::get_record(store, &id, Some(metrics_registry.clone())).await {
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
    store: Arc<LmdbStore>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
    backend: WriteBackend,
) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
    let start = std::time::Instant::now();
    let (endpoint, method) = ("/records/{id}", "PUT");

    let result = match backend {
        WriteBackend::Local => {
            update::update_record(store, &id, update_request, Some(metrics_registry.clone()))
                .await
                .map_err(|e| (status_for(e.to_status_code()), e.to_string()))
        }
        #[cfg(feature = "kubernetes")]
        WriteBackend::Kubernetes(ref client) => client
            .update(&id, &update_request)
            .await
            .map_err(|e| (status_for(e.to_status_code()), e.to_string())),
    };

    let reply: Box<dyn warp::Reply> = match result {
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
        Err((code, e)) => {
            record_api_metrics(
                endpoint,
                method,
                code.as_u16(),
                start.elapsed().as_secs_f64(),
                &metrics_registry,
            )
            .await;
            Box::new(warp::reply::with_status(
                warp::reply::json(&update::ApiResponse::<update::DnsRecord>::error(e)),
                code,
            ))
        }
    };
    Ok(reply)
}

async fn delete_record_handler(
    id: String,
    store: Arc<LmdbStore>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
    backend: WriteBackend,
) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
    let start = std::time::Instant::now();
    let (endpoint, method) = ("/records/{id}", "DELETE");

    let result = match backend {
        WriteBackend::Local => update::delete_record(store, &id, Some(metrics_registry.clone()))
            .await
            .map_err(|e| (status_for(e.to_status_code()), e.to_string())),
        #[cfg(feature = "kubernetes")]
        WriteBackend::Kubernetes(ref client) => client
            .delete(&id)
            .await
            .map_err(|e| (status_for(e.to_status_code()), e.to_string())),
    };

    let reply: Box<dyn warp::Reply> = match result {
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
                warp::reply(),
                warp::http::StatusCode::NO_CONTENT,
            ))
        }
        Err((code, e)) => {
            record_api_metrics(
                endpoint,
                method,
                code.as_u16(),
                start.elapsed().as_secs_f64(),
                &metrics_registry,
            )
            .await;
            Box::new(warp::reply::with_status(
                warp::reply::json(&update::ApiResponse::<()>::error(e)),
                code,
            ))
        }
    };
    Ok(reply)
}

async fn list_records_handler(
    query_params: std::collections::HashMap<String, String>,
    store: Arc<LmdbStore>,
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
        match update::list_records(store, page, per_page, Some(metrics_registry.clone())).await {
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
    store: Arc<LmdbStore>,
    metrics_registry: Arc<RwLock<MetricsRegistry>>,
    backend: WriteBackend,
) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
    let start = std::time::Instant::now();
    let (endpoint, method) = ("/records", "POST");

    let result = match backend {
        WriteBackend::Local => update::create_record_from_request(
            store,
            create_request,
            Some(metrics_registry.clone()),
        )
        .await
        .map_err(|e| (status_for(e.to_status_code()), e.to_string())),
        #[cfg(feature = "kubernetes")]
        WriteBackend::Kubernetes(ref client) => client
            .create(&create_request)
            .await
            .map_err(|e| (status_for(e.to_status_code()), e.to_string())),
    };

    let reply: Box<dyn warp::Reply> = match result {
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
        Err((code, e)) => {
            record_api_metrics(
                endpoint,
                method,
                code.as_u16(),
                start.elapsed().as_secs_f64(),
                &metrics_registry,
            )
            .await;
            Box::new(warp::reply::with_status(
                warp::reply::json(&update::ApiResponse::<update::DnsRecord>::error(e)),
                code,
            ))
        }
    };
    Ok(reply)
}
