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

fn setup_file_logging() -> Result<(), Box<dyn std::error::Error>> {
    // Create logs directory if it doesn't exist
    fs::create_dir_all("logs")?;
    
    // Generate timestamp for log filename
    let now: DateTime<Utc> = Utc::now();
    let timestamp = now.format("%Y-%m-%d_%H-%M-%S");
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

    // Load DNS records and create shared references
    let records = update::load_records(DNS_RECORDS_FILE);
    let records_for_filter = Arc::clone(&records);
    let records_for_server = Arc::clone(&records);

    let records_filter = warp::any().map(move || Arc::clone(&records_for_filter));
    
    // API route for updating DNS records
    let update_route = warp::path("update")
        .and(warp::post())
        .and(warp::body::json())
        .and(records_filter.clone())
        .map(|new_record: update::DnsRecord, records: Arc<RwLock<update::DnsRecords>>| {
            tokio::spawn(async move {
                update::update_record(records, new_record).await;
            });
            warp::reply::reply()
        });

    // Start metrics server in background
    let metrics_addr = format!("127.0.0.1:{}", metrics_port);
    let metrics_server = metrics::MetricsServer::new(Arc::clone(&metrics_registry));
    let metrics_addr_clone = metrics_addr.clone();
    
    tokio::spawn(async move {
        let addr = metrics_addr_clone.parse::<std::net::SocketAddr>().unwrap();
        if let Err(e) = metrics_server.start(addr).await {
            error!("Metrics server failed to start: {}", e);
        }
    });

    info!("Metrics server listening on http://{}/metrics", metrics_addr);

    // Start API server in background
    let api_addr_clone = api_addr.clone();
    let api_server = async move {
        warp::serve(update_route).run(api_addr_clone.parse::<std::net::SocketAddr>().unwrap()).await;
    };

    info!("API server listening on {}", api_addr);
    tokio::spawn(api_server);

    // Run DNS server
    if let Err(e) = server::run(&addr, records_for_server, metrics_registry).await {
        error!("Server error: {}", e);
    }
}