use warp::Filter;
use std::sync::Arc;
use tokio::sync::RwLock;
use log::{info, error};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};
use std::fs;
use chrono::{DateTime, Utc};

mod server;
mod packet;
mod query;
mod update;
mod metrics;

const DNS_RECORDS_FILE: &str = "dns_records.txt";

// Helper function to record API metrics
async fn record_api_metrics(
    endpoint: &str,
    method: &str,
    status_code: u16,
    duration: f64,
    metrics_registry: &Arc<RwLock<metrics::MetricsRegistry>>,
) {
    let registry = metrics_registry.read().await;
    let status_str = status_code.to_string();
    registry.dns_metrics().record_api_request(endpoint, method, &status_str, duration);
    
    // Record error if status code indicates an error
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
        registry.dns_metrics().record_api_error(endpoint, error_type);
    }
}

// Handler for GET /records/{id} endpoint
async fn get_record_handler(
    id: String,
    records: Arc<RwLock<update::DnsRecords>>,
    metrics_registry: Arc<RwLock<metrics::MetricsRegistry>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let start_time = std::time::Instant::now();
    let endpoint = "/records/{id}";
    let method = "GET";
    
    let result = match update::get_record(records, &id, Some(metrics_registry.clone())).await {
        Ok(record) => {
            let response = update::ApiResponse::success(record);
            let reply = warp::reply::with_status(
                warp::reply::json(&response),
                warp::http::StatusCode::OK,
            );
            
            // Record metrics
            let duration = start_time.elapsed().as_secs_f64();
            record_api_metrics(endpoint, method, 200, duration, &metrics_registry).await;
            
            Ok(reply)
        }
        Err(e) => {
            let response = update::ApiResponse::<update::DnsRecord>::error(e.to_string());
            let status_code = match e.to_status_code() {
                404 => warp::http::StatusCode::NOT_FOUND,
                400 => warp::http::StatusCode::BAD_REQUEST,
                409 => warp::http::StatusCode::CONFLICT,
                500 => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            let reply = warp::reply::with_status(
                warp::reply::json(&response),
                status_code,
            );
            
            // Record metrics
            let duration = start_time.elapsed().as_secs_f64();
            record_api_metrics(endpoint, method, status_code.as_u16(), duration, &metrics_registry).await;
            
            Ok(reply)
        }
    };
    
    result
}

// Handler for PUT /records/{id} endpoint
async fn update_record_handler(
    id: String,
    update_request: update::UpdateRecordRequest,
    records: Arc<RwLock<update::DnsRecords>>,
    metrics_registry: Arc<RwLock<metrics::MetricsRegistry>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let start_time = std::time::Instant::now();
    let endpoint = "/records/{id}";
    let method = "PUT";
    
    let result = match update::update_record(records, &id, update_request, Some(metrics_registry.clone())).await {
        Ok(record) => {
            let response = update::ApiResponse::success(record);
            let reply = warp::reply::with_status(
                warp::reply::json(&response),
                warp::http::StatusCode::OK,
            );
            
            // Record metrics
            let duration = start_time.elapsed().as_secs_f64();
            record_api_metrics(endpoint, method, 200, duration, &metrics_registry).await;
            
            Ok(reply)
        }
        Err(e) => {
            let response = update::ApiResponse::<update::DnsRecord>::error(e.to_string());
            let status_code = match e.to_status_code() {
                404 => warp::http::StatusCode::NOT_FOUND,
                400 => warp::http::StatusCode::BAD_REQUEST,
                409 => warp::http::StatusCode::CONFLICT,
                500 => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            let reply = warp::reply::with_status(
                warp::reply::json(&response),
                status_code,
            );
            
            // Record metrics
            let duration = start_time.elapsed().as_secs_f64();
            record_api_metrics(endpoint, method, status_code.as_u16(), duration, &metrics_registry).await;
            
            Ok(reply)
        }
    };
    
    result
}

// Handler for DELETE /records/{id} endpoint
async fn delete_record_handler(
    id: String,
    records: Arc<RwLock<update::DnsRecords>>,
    metrics_registry: Arc<RwLock<metrics::MetricsRegistry>>,
) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
    let start_time = std::time::Instant::now();
    let endpoint = "/records/{id}";
    let method = "DELETE";
    
    let result = match update::delete_record(records, &id, Some(metrics_registry.clone())).await {
        Ok(()) => {
            // Return HTTP 204 No Content on successful deletion
            let response = update::ApiResponse::<()>::success(());
            let reply = Box::new(warp::reply::with_status(
                warp::reply::json(&response),
                warp::http::StatusCode::NO_CONTENT,
            )) as Box<dyn warp::Reply>;
            
            // Record metrics
            let duration = start_time.elapsed().as_secs_f64();
            record_api_metrics(endpoint, method, 204, duration, &metrics_registry).await;
            
            Ok(reply)
        }
        Err(e) => {
            let response = update::ApiResponse::<()>::error(e.to_string());
            let status_code = match e.to_status_code() {
                404 => warp::http::StatusCode::NOT_FOUND,
                400 => warp::http::StatusCode::BAD_REQUEST,
                409 => warp::http::StatusCode::CONFLICT,
                500 => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            let reply = Box::new(warp::reply::with_status(
                warp::reply::json(&response),
                status_code,
            )) as Box<dyn warp::Reply>;
            
            // Record metrics
            let duration = start_time.elapsed().as_secs_f64();
            record_api_metrics(endpoint, method, status_code.as_u16(), duration, &metrics_registry).await;
            
            Ok(reply)
        }
    };
    
    result
}

// Handler for GET /records endpoint with pagination
async fn list_records_handler(
    query_params: std::collections::HashMap<String, String>,
    records: Arc<RwLock<update::DnsRecords>>,
    metrics_registry: Arc<RwLock<metrics::MetricsRegistry>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let start_time = std::time::Instant::now();
    let endpoint = "/records";
    let method = "GET";
    // Parse pagination parameters with defaults
    let page = query_params
        .get("page")
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(1);
    
    let per_page = query_params
        .get("per_page")
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(50);

    let result = match update::list_records(records, page, per_page, Some(metrics_registry.clone())).await {
        Ok(record_list) => {
            let response = update::ApiResponse::success(record_list);
            let reply = warp::reply::with_status(
                warp::reply::json(&response),
                warp::http::StatusCode::OK,
            );
            
            // Record metrics
            let duration = start_time.elapsed().as_secs_f64();
            record_api_metrics(endpoint, method, 200, duration, &metrics_registry).await;
            
            Ok(reply)
        }
        Err(e) => {
            let response = update::ApiResponse::<update::RecordListResponse>::error(e.to_string());
            let status_code = match e.to_status_code() {
                404 => warp::http::StatusCode::NOT_FOUND,
                400 => warp::http::StatusCode::BAD_REQUEST,
                409 => warp::http::StatusCode::CONFLICT,
                500 => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            let reply = warp::reply::with_status(
                warp::reply::json(&response),
                status_code,
            );
            
            // Record metrics
            let duration = start_time.elapsed().as_secs_f64();
            record_api_metrics(endpoint, method, status_code.as_u16(), duration, &metrics_registry).await;
            
            Ok(reply)
        }
    };
    
    result
}

// Handler for POST /records endpoint for new record creation
async fn create_record_handler(
    create_request: update::CreateRecordRequest,
    records: Arc<RwLock<update::DnsRecords>>,
    metrics_registry: Arc<RwLock<metrics::MetricsRegistry>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let start_time = std::time::Instant::now();
    let endpoint = "/records";
    let method = "POST";
    
    let result = match update::create_record_from_request(records, create_request, Some(metrics_registry.clone())).await {
        Ok(record) => {
            let response = update::ApiResponse::success(record);
            let reply = warp::reply::with_status(
                warp::reply::json(&response),
                warp::http::StatusCode::CREATED,
            );
            
            // Record metrics
            let duration = start_time.elapsed().as_secs_f64();
            record_api_metrics(endpoint, method, 201, duration, &metrics_registry).await;
            
            Ok(reply)
        }
        Err(e) => {
            let response = update::ApiResponse::<update::DnsRecord>::error(e.to_string());
            let status_code = match e.to_status_code() {
                404 => warp::http::StatusCode::NOT_FOUND,
                400 => warp::http::StatusCode::BAD_REQUEST,
                409 => warp::http::StatusCode::CONFLICT,
                500 => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            let reply = warp::reply::with_status(
                warp::reply::json(&response),
                status_code,
            );
            
            // Record metrics
            let duration = start_time.elapsed().as_secs_f64();
            record_api_metrics(endpoint, method, status_code.as_u16(), duration, &metrics_registry).await;
            
            Ok(reply)
        }
    };
    
    result
}

fn setup_file_logging() -> Result<(), Box<dyn std::error::Error>> {
    // Create logs directory if it doesn't exist
    fs::create_dir_all("logs")?;
    
    // Generate timestamp for log filename
    let now: DateTime<Utc> = Utc::now();
    let timestamp = now.format("%Y-%m-%d_%H");
    let log_filename = format!("logs/rind_{}.log", timestamp);
    
    // Create file appender
    let file_appender = tracing_appender::rolling::never("logs", format!("rind_{}.log", timestamp));
    
    // Set up log level from environment variable
    let log_level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&log_level));
    
    // Determine log format from environment variable
    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());
    
    if log_format == "json" {
        // JSON format for production
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer()
                .json()
                .with_writer(file_appender)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true))
            .init();
    } else {
        // Human-readable format for development
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer()
                .with_writer(file_appender)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .with_ansi(false)) // Disable colors for file output
            .init();
    }
    
    println!("Logging initialized - writing to: {}", log_filename);
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = setup_file_logging() {
        eprintln!("Failed to initialize logging: {}", e);
        std::process::exit(1);
    }

    let addr = std::env::var("DNS_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:12312".to_string());
    let api_addr = std::env::var("API_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let metrics_port = std::env::var("METRICS_PORT")
        .unwrap_or_else(|_| "9090".to_string())
        .parse::<u16>()
        .unwrap_or(9090);
    let server_id = std::env::var("SERVER_ID").unwrap_or_else(|_| {
        format!("dns-server-{}", std::process::id())
    });

    info!("Starting DNS server with server ID: {}", server_id);
    
    // Initialize metrics registry
    let metrics_registry = match metrics::MetricsRegistry::new() {
        Ok(registry) => Arc::new(RwLock::new(registry)),
        Err(e) => {
            error!("Failed to initialize metrics registry: {}", e);
            error!("Continuing without metrics collection");
            // Create a dummy registry that won't be used
            Arc::new(RwLock::new(metrics::MetricsRegistry::new().unwrap()))
        }
    };

    // Ensure datastore is initialized before loading records
    if let Err(e) = update::ensure_datastore_initialized(DNS_RECORDS_FILE) {
        error!("Failed to initialize datastore: {}", e);
        std::process::exit(1);
    }
    
    // Load DNS records and create shared references
    let records = update::load_records(DNS_RECORDS_FILE);
    let records_for_filter = Arc::clone(&records);
    let records_for_server = Arc::clone(&records);

    // Initialize active records count metric
    {
        let records_guard = records.read().await;
        let metrics_guard = metrics_registry.read().await;
        metrics_guard.dns_metrics().set_active_records_count(records_guard.len() as f64);
        info!("Initialized active records count: {}", records_guard.len());
    }

    let records_filter = warp::any().map(move || Arc::clone(&records_for_filter));
    
    // Create metrics filter
    let metrics_for_filter = Arc::clone(&metrics_registry);
    let metrics_filter = warp::any().map(move || Arc::clone(&metrics_for_filter));
    
    // API route for updating DNS records (legacy)
    let update_route = warp::path("update")
        .and(warp::post())
        .and(warp::body::json())
        .and(records_filter.clone())
        .map(|new_record: update::DnsRecord, records: Arc<RwLock<update::DnsRecords>>| {
            tokio::spawn(async move {
                update::update_record_legacy(records, new_record).await;
            });
            warp::reply::reply()
        });

    // GET /records/{id} - Retrieve a specific record by ID
    let get_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::get())
        .and(records_filter.clone())
        .and(metrics_filter.clone())
        .and_then(get_record_handler);

    // PUT /records/{id} - Update an existing record by ID
    let update_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::put())
        .and(warp::body::json())
        .and(records_filter.clone())
        .and(metrics_filter.clone())
        .and_then(update_record_handler);

    // DELETE /records/{id} - Delete a record by ID
    let delete_record_route = warp::path("records")
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::delete())
        .and(records_filter.clone())
        .and(metrics_filter.clone())
        .and_then(delete_record_handler);

    // GET /records - List all records with pagination
    let list_records_route = warp::path("records")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .and(records_filter.clone())
        .and(metrics_filter.clone())
        .and_then(list_records_handler);

    // POST /records - Create new record
    let create_record_route = warp::path("records")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(records_filter.clone())
        .and(metrics_filter.clone())
        .and_then(create_record_handler);

    // Start metrics server in background
    let metrics_addr = format!("0.0.0.0:{}", metrics_port);
    let metrics_server = metrics::MetricsServer::new(Arc::clone(&metrics_registry));
    let metrics_addr_clone = metrics_addr.clone();
    
    tokio::spawn(async move {
        let addr = metrics_addr_clone.parse::<std::net::SocketAddr>().unwrap();
        if let Err(e) = metrics_server.start(addr).await {
            error!("Metrics server failed to start: {}", e);
        }
    });

    info!("Metrics server listening on http://{}/metrics", metrics_addr);

    // Combine all API routes
    let api_routes = update_route
        .or(get_record_route)
        .or(update_record_route)
        .or(delete_record_route)
        .or(list_records_route)
        .or(create_record_route);

    // Start API server in background
    let api_addr_clone = api_addr.clone();
    let api_server = async move {
        warp::serve(api_routes).run(api_addr_clone.parse::<std::net::SocketAddr>().unwrap()).await;
    };

    info!("API server listening on {}", api_addr);
    tokio::spawn(api_server);

    // Run DNS server
    if let Err(e) = server::run(&addr, records_for_server, metrics_registry).await {
        error!("Server error: {}", e);
    }
}