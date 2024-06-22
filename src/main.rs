use warp::Filter;
use std::sync::Arc;
use tokio::sync::RwLock;
use log::{info, debug, error};

mod server;
mod packet;
mod query;
mod update;

const DNS_RECORDS_FILE: &str = "dns_records.txt";

#[tokio::main]
async fn main() {
    env_logger::init();

    let addr = "127.0.0.1:12312";
    let api_addr = "127.0.0.1:8080";
    
    let records = update::load_records(DNS_RECORDS_FILE);
    let records_for_filter = Arc::clone(&records); // Clone Arc for use in warp filter
    let records_for_server = Arc::clone(&records); // Clone Arc for use in server

    let records_filter = warp::any().map(move || Arc::clone(&records_for_filter));
    
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

    // Use a tokio task to run the API server
    let api_server = async {
        warp::serve(update_route).run(api_addr.parse::<std::net::SocketAddr>().unwrap()).await;
    };

    // Log that the API server has successfully started
    info!("API server listening on {}", api_addr);

    // Spawn the API server task
    tokio::spawn(api_server);

    // Run the DNS server
    if let Err(e) = server::run(addr, records_for_server).await {
        error!("Server error: {}", e);
    }
}

