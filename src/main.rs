use warp::Filter;
use std::sync::Arc;
use tokio::sync::RwLock;
use log::{info, error};

mod server;
mod packet;
mod query;
mod update;
mod metrics;

const DNS_RECORDS_FILE: &str = "dns_records.txt";

#[tokio::main]
async fn main() {
    env_logger::init();

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