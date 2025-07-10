use prometheus::{
    Counter, CounterVec, Gauge, HistogramVec, Opts, Registry, TextEncoder,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server, StatusCode,
};
use std::convert::Infallible;
use std::net::SocketAddr;
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

/// Configuration for logging setup
pub struct LogConfig {
    pub level: String,
    pub format: LogFormat,
}

/// Log output format options
#[derive(Debug, Clone)]
pub enum LogFormat {
    Json,
    Text,
}

impl Default for LogConfig {
    fn default() -> Self {
        LogConfig {
            level: "info".to_string(),
            format: LogFormat::Json,
        }
    }
}

impl LogConfig {
    /// Create log config from environment variables
    pub fn from_env() -> Self {
        let level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        let format = match std::env::var("LOG_FORMAT").as_deref() {
            Ok("text") => LogFormat::Text,
            Ok("json") => LogFormat::Json,
            _ => {
                // Default to JSON in production-like environments
                if std::env::var("RUST_ENV").as_deref() == Ok("production") {
                    LogFormat::Json
                } else {
                    LogFormat::Text
                }
            }
        };

        LogConfig { level, format }
    }

    /// Initialize tracing subscriber with the configured settings
    pub fn init_tracing(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let env_filter = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(&self.level))?;

        match self.format {
            LogFormat::Json => {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt::layer().json())
                    .try_init()?;
            }
            LogFormat::Text => {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt::layer().pretty())
                    .try_init()?;
            }
        }

        Ok(())
    }
}

/// DNS-specific metrics collector
pub struct DnsMetrics {
    // Query metrics
    pub queries_total: CounterVec,
    pub query_duration: HistogramVec,
    pub queries_per_second: Gauge,
    
    // Response metrics
    pub responses_total: CounterVec,
    pub nxdomain_total: Counter,
    pub servfail_total: Counter,
    
    // System metrics
    pub server_uptime: Gauge,
    pub active_connections: Gauge,
    pub packet_errors_total: Counter,
}

impl DnsMetrics {
    /// Create a new DnsMetrics instance with all metric definitions
    pub fn new() -> Result<Self, prometheus::Error> {
        let queries_total = CounterVec::new(
            Opts::new("dns_queries_total", "Total number of DNS queries by type"),
            &["query_type", "instance"]
        )?;

        let query_duration = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "dns_query_duration_seconds",
                "DNS query processing duration in seconds"
            ),
            &["query_type", "instance"]
        )?;

        let queries_per_second = Gauge::new(
            "dns_queries_per_second",
            "Current DNS queries per second rate"
        )?;

        let responses_total = CounterVec::new(
            Opts::new("dns_responses_total", "Total number of DNS responses by code"),
            &["response_code", "instance"]
        )?;

        let nxdomain_total = Counter::new(
            "dns_nxdomain_total",
            "Total number of NXDOMAIN responses"
        )?;

        let servfail_total = Counter::new(
            "dns_servfail_total",
            "Total number of SERVFAIL responses"
        )?;

        let server_uptime = Gauge::new(
            "dns_server_uptime_seconds",
            "DNS server uptime in seconds"
        )?;

        let active_connections = Gauge::new(
            "dns_active_connections",
            "Number of active DNS connections"
        )?;

        let packet_errors_total = Counter::new(
            "dns_packet_errors_total",
            "Total number of DNS packet parsing errors"
        )?;

        Ok(DnsMetrics {
            queries_total,
            query_duration,
            queries_per_second,
            responses_total,
            nxdomain_total,
            servfail_total,
            server_uptime,
            active_connections,
            packet_errors_total,
        })
    }

    /// Register all metrics with the provided registry
    pub fn register(&self, registry: &Registry) -> Result<(), prometheus::Error> {
        registry.register(Box::new(self.queries_total.clone()))?;
        registry.register(Box::new(self.query_duration.clone()))?;
        registry.register(Box::new(self.queries_per_second.clone()))?;
        registry.register(Box::new(self.responses_total.clone()))?;
        registry.register(Box::new(self.nxdomain_total.clone()))?;
        registry.register(Box::new(self.servfail_total.clone()))?;
        registry.register(Box::new(self.server_uptime.clone()))?;
        registry.register(Box::new(self.active_connections.clone()))?;
        registry.register(Box::new(self.packet_errors_total.clone()))?;
        Ok(())
    }
}

/// Global metrics registry and HTTP server
pub struct MetricsRegistry {
    registry: Registry,
    dns_metrics: DnsMetrics,
}

impl MetricsRegistry {
    /// Create a new metrics registry with DNS metrics
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();
        let dns_metrics = DnsMetrics::new()?;
        dns_metrics.register(&registry)?;

        Ok(MetricsRegistry {
            registry,
            dns_metrics,
        })
    }

    /// Get reference to DNS metrics
    pub fn dns_metrics(&self) -> &DnsMetrics {
        &self.dns_metrics
    }

    /// Get metrics in Prometheus text format
    pub fn gather_metrics(&self) -> Result<String, prometheus::Error> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder.encode_to_string(&metric_families)
    }
}

/// HTTP server for exposing metrics endpoint
pub struct MetricsServer {
    registry: Arc<RwLock<MetricsRegistry>>,
}

impl MetricsServer {
    /// Create a new metrics server
    pub fn new(registry: Arc<RwLock<MetricsRegistry>>) -> Self {
        MetricsServer { registry }
    }

    /// Start the metrics HTTP server on the specified address
    pub async fn start(&self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let registry = self.registry.clone();
        
        let make_svc = make_service_fn(move |_conn| {
            let registry = registry.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let registry = registry.clone();
                    async move {
                        handle_metrics_request(req, registry).await
                    }
                }))
            }
        });

        let server = Server::bind(&addr).serve(make_svc);
        
        log::info!("Metrics server starting on http://{}/metrics", addr);
        
        if let Err(e) = server.await {
            log::error!("Metrics server error: {}", e);
            return Err(Box::new(e));
        }

        Ok(())
    }
}

/// Handle HTTP requests to the metrics endpoint
async fn handle_metrics_request(
    req: Request<Body>,
    registry: Arc<RwLock<MetricsRegistry>>,
) -> Result<Response<Body>, Infallible> {
    let response = match req.uri().path() {
        "/metrics" => {
            let registry = registry.read().await;
            match registry.gather_metrics() {
                Ok(metrics) => {
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", "text/plain; version=0.0.4")
                        .body(Body::from(metrics))
                        .unwrap_or_else(|_| {
                            Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from("Failed to build response"))
                                .unwrap()
                        })
                }
                Err(e) => {
                    log::error!("Failed to gather metrics: {}", e);
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Failed to gather metrics"))
                        .unwrap()
                }
            }
        }
        _ => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))
                .unwrap()
        }
    };
    
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dns_metrics_creation() {
        let metrics = DnsMetrics::new();
        assert!(metrics.is_ok());
    }

    #[test]
    fn test_metrics_registry_creation() {
        let registry = MetricsRegistry::new();
        assert!(registry.is_ok());
    }

    #[test]
    fn test_metrics_gathering() {
        let registry = MetricsRegistry::new().unwrap();
        let metrics_text = registry.gather_metrics();
        assert!(metrics_text.is_ok());
        
        let text = metrics_text.unwrap();
        // Check for metrics that are always present (Gauge and Counter types)
        assert!(text.contains("dns_server_uptime_seconds"));
        assert!(text.contains("dns_nxdomain_total"));
        assert!(text.contains("dns_servfail_total"));
        assert!(text.contains("dns_packet_errors_total"));
    }

    #[test]
    fn test_log_config_from_env() {
        // Test default configuration
        let config = LogConfig::default();
        assert_eq!(config.level, "info");
        matches!(config.format, LogFormat::Json);
        
        // Test environment variable parsing
        std::env::set_var("LOG_LEVEL", "debug");
        std::env::set_var("LOG_FORMAT", "text");
        let config = LogConfig::from_env();
        assert_eq!(config.level, "debug");
        matches!(config.format, LogFormat::Text);
        
        // Clean up
        std::env::remove_var("LOG_LEVEL");
        std::env::remove_var("LOG_FORMAT");
    }
}