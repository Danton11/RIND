use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use log::{error, info};
use rind::instance::{build_instance, InstanceConfig};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Directory that holds the LMDB environment. `RIND_LMDB_PATH` wins;
/// otherwise fall back to `$DATA_DIR/lmdb`.
fn get_lmdb_path() -> PathBuf {
    if let Ok(p) = std::env::var("RIND_LMDB_PATH") {
        return PathBuf::from(p);
    }
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(data_dir).join("lmdb")
}

fn setup_logging() -> Result<(), Box<dyn std::error::Error>> {
    let log_level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&log_level));

    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());
    let disable_file_logging = std::env::var("DISABLE_FILE_LOGGING")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    if disable_file_logging {
        if log_format == "json" {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .json()
                        .with_writer(std::io::stdout)
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_file(true)
                        .with_line_number(true),
                )
                .init();
        } else {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .with_writer(std::io::stdout)
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_file(true)
                        .with_line_number(true)
                        .with_ansi(true),
                )
                .init();
        }
        println!("Logging initialized - writing to stdout");
    } else {
        fs::create_dir_all("logs")?;
        let now: DateTime<Utc> = Utc::now();
        let timestamp = now.format("%Y-%m-%d_%H");
        let log_filename = format!("logs/rind_{}.log", timestamp);
        let file_appender =
            tracing_appender::rolling::never("logs", format!("rind_{}.log", timestamp));

        if log_format == "json" {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .json()
                        .with_writer(file_appender)
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_file(true)
                        .with_line_number(true),
                )
                .init();
        } else {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .with_writer(file_appender)
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_file(true)
                        .with_line_number(true)
                        .with_ansi(false),
                )
                .init();
        }

        println!("Logging initialized - writing to: {}", log_filename);
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = setup_logging() {
        eprintln!("Failed to initialize logging: {}", e);
        std::process::exit(1);
    }

    let dns_bind: SocketAddr = std::env::var("DNS_BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:12312".to_string())
        .parse()
        .expect("DNS_BIND_ADDR must be a valid socket address");
    let api_bind: SocketAddr = std::env::var("API_BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        .parse()
        .expect("API_BIND_ADDR must be a valid socket address");
    let metrics_port = std::env::var("METRICS_PORT")
        .unwrap_or_else(|_| "9090".to_string())
        .parse::<u16>()
        .unwrap_or(9090);
    let metrics_bind: SocketAddr = format!("0.0.0.0:{}", metrics_port)
        .parse()
        .expect("metrics addr");
    let server_id =
        std::env::var("SERVER_ID").unwrap_or_else(|_| format!("dns-server-{}", std::process::id()));

    info!("Starting DNS server with server ID: {}", server_id);

    let cfg = InstanceConfig {
        dns_bind,
        api_bind,
        lmdb_path: get_lmdb_path(),
        server_id,
        metrics_bind: Some(metrics_bind),
    };

    let instance = match build_instance(cfg).await {
        Ok(i) => i,
        Err(e) => {
            error!("Failed to build instance: {}", e);
            std::process::exit(1);
        }
    };

    // Block until the DNS dispatch loop exits (it only exits on channel
    // close, which means the receiver task died). Any task error is
    // already logged inside instance.rs.
    if let Err(e) = instance.dns_task.await {
        error!("DNS task join error: {}", e);
    }
}
