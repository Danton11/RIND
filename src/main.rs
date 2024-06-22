mod server;
mod packet;
mod query;
use env_logger;
use log::{info, warn, error, debug};

#[tokio::main]
async fn main() {
    env_logger::init();

    // Define host and port
    let host = "127.0.0.1";
    let port = 12312;
    let addr = format!("{}:{}", host, port);

    // Start the DNS server
    if let Err(e) = server::run(&addr).await {
        error!("Server error: {}", e);
    } else {
        info!("RIND starting on: {}", &addr)
    }
}

