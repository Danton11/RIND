use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::TryStreamExt;
use kube::api::{Api, ListParams, Patch, PatchParams};
use kube::runtime::watcher::{self, Event};
use kube::runtime::WatchStreamExt;
use kube::Client;
use prometheus::{CounterVec, Gauge, Opts};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::crd::{self, DnsRecord, DnsRecordStatus};
use crate::metrics::MetricsRegistry;
use crate::storage::LmdbStore;

pub struct WatcherMetrics {
    pub events_total: CounterVec,
    pub errors_total: CounterVec,
    pub last_sync_timestamp: Gauge,
    pub synced_records: Gauge,
}

impl WatcherMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let events_total = CounterVec::new(
            Opts::new(
                "rind_watcher_events_total",
                "Total CRD watcher events processed",
            ),
            &["event_type"],
        )?;
        let errors_total = CounterVec::new(
            Opts::new("rind_watcher_errors_total", "Total CRD watcher errors"),
            &["error_type"],
        )?;
        let last_sync_timestamp = Gauge::new(
            "rind_watcher_last_sync_timestamp",
            "Unix timestamp of last successful full sync",
        )?;
        let synced_records = Gauge::new(
            "rind_watcher_synced_records",
            "Number of records synced in last full sync",
        )?;

        Ok(Self {
            events_total,
            errors_total,
            last_sync_timestamp,
            synced_records,
        })
    }

    pub fn register(&self, registry: &prometheus::Registry) -> Result<(), prometheus::Error> {
        registry.register(Box::new(self.events_total.clone()))?;
        registry.register(Box::new(self.errors_total.clone()))?;
        registry.register(Box::new(self.last_sync_timestamp.clone()))?;
        registry.register(Box::new(self.synced_records.clone()))?;
        Ok(())
    }
}

pub struct CrdWatcher {
    store: Arc<LmdbStore>,
    api: Api<DnsRecord>,
    namespace: String,
    metrics: Arc<RwLock<MetricsRegistry>>,
    watcher_metrics: WatcherMetrics,
    resync_interval: Duration,
    /// Flipped to `true` after the first successful full sync. Read by the
    /// `/health` route so fresh pods don't accept traffic before LMDB has
    /// caught up with etcd.
    ready: Arc<AtomicBool>,
}

#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("kube error: {0}")]
    Kube(#[from] kube::Error),
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("conversion error: {0}")]
    Conversion(#[from] crd::ConversionError),
    #[error("watcher stream error: {0}")]
    Watcher(#[from] kube::runtime::watcher::Error),
    #[error("metrics error: {0}")]
    Metrics(#[from] prometheus::Error),
    #[error("watch stream ended unexpectedly")]
    StreamEnded,
}

impl CrdWatcher {
    pub fn new(
        store: Arc<LmdbStore>,
        client: Client,
        namespace: String,
        metrics: Arc<RwLock<MetricsRegistry>>,
        ready: Arc<AtomicBool>,
    ) -> Result<Self, WatcherError> {
        let resync_secs: u64 = match std::env::var("RIND_RESYNC_INTERVAL_SECS") {
            Ok(v) => v.parse().unwrap_or_else(|_| {
                warn!(
                    value = %v,
                    "RIND_RESYNC_INTERVAL_SECS is not a valid u64, falling back to 300"
                );
                300
            }),
            Err(_) => 300,
        };

        let watcher_metrics = WatcherMetrics::new()?;
        let api = Api::namespaced(client, &namespace);

        Ok(Self {
            store,
            api,
            namespace,
            metrics,
            watcher_metrics,
            resync_interval: Duration::from_secs(resync_secs),
            ready,
        })
    }

    /// Register watcher-specific Prometheus metrics.
    pub async fn register_metrics(&self) -> Result<(), WatcherError> {
        let registry_guard = self.metrics.read().await;
        let registry = registry_guard.registry();
        self.watcher_metrics.register(registry)?;
        Ok(())
    }

    /// Run the watcher loop. This blocks indefinitely, processing CRD events
    /// and syncing them to the local LMDB store.
    pub async fn run(&self) -> Result<(), WatcherError> {
        let api = &self.api;

        info!(namespace = %self.namespace, "Starting CRD watcher, performing initial sync");
        self.full_sync(api).await?;
        self.ready.store(true, Ordering::Relaxed);

        info!("Initial sync complete, switching to incremental watch");

        let resync_interval = self.resync_interval;
        let api_for_resync = api.clone();

        // Spawn a periodic resync task as a safety net
        let store_resync = Arc::clone(&self.store);
        let watcher_metrics_events = self.watcher_metrics.events_total.clone();
        let watcher_metrics_errors = self.watcher_metrics.errors_total.clone();
        let watcher_metrics_sync_ts = self.watcher_metrics.last_sync_timestamp.clone();
        let watcher_metrics_synced = self.watcher_metrics.synced_records.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(resync_interval);
            interval.tick().await; // skip immediate first tick
            loop {
                interval.tick().await;
                info!("Periodic resync triggered");
                match api_for_resync.list(&ListParams::default()).await {
                    Ok(list) => {
                        let mut records = Vec::with_capacity(list.items.len());
                        for cr in &list.items {
                            let id = cr.metadata.name.as_deref().unwrap_or_default();
                            match cr.spec.to_dns_record(id) {
                                Ok(record) => records.push(record),
                                Err(e) => {
                                    warn!(resource = id, error = %e, "Skipping CRD during resync");
                                }
                            }
                        }
                        if let Err(e) = store_resync.put_records_batch(&records) {
                            error!(error = %e, "Periodic resync failed to write to LMDB");
                            watcher_metrics_errors
                                .with_label_values(&["resync_storage"])
                                .inc();
                        } else {
                            let now = chrono::Utc::now().timestamp() as f64;
                            watcher_metrics_sync_ts.set(now);
                            watcher_metrics_synced.set(records.len() as f64);
                            watcher_metrics_events.with_label_values(&["resync"]).inc();
                            info!(count = records.len(), "Periodic resync complete");
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Periodic resync failed to list CRDs");
                        watcher_metrics_errors
                            .with_label_values(&["resync_kube"])
                            .inc();
                    }
                }
            }
        });

        // Main watcher stream with automatic backoff on errors
        let watcher_config = watcher::Config::default();
        let mut stream = Box::pin(watcher::watcher(api.clone(), watcher_config).default_backoff());

        while let Some(event) = stream.try_next().await? {
            if let Err(e) = self.handle_event(event).await {
                error!(error = %e, "Error handling watcher event");
                self.watcher_metrics
                    .errors_total
                    .with_label_values(&["event_handling"])
                    .inc();
            }
        }

        Err(WatcherError::StreamEnded)
    }

    async fn full_sync(&self, api: &Api<DnsRecord>) -> Result<(), WatcherError> {
        let list = api.list(&ListParams::default()).await?;
        let mut records = Vec::with_capacity(list.items.len());

        for cr in &list.items {
            let id = cr.metadata.name.as_deref().unwrap_or_default();
            match cr.spec.to_dns_record(id) {
                Ok(record) => records.push(record),
                Err(e) => {
                    warn!(
                        resource = id,
                        error = %e,
                        "Skipping CRD with conversion error during full sync"
                    );
                    self.watcher_metrics
                        .errors_total
                        .with_label_values(&["conversion"])
                        .inc();
                }
            }
        }

        info!(count = records.len(), "Syncing records to local LMDB");
        self.store.put_records_batch(&records)?;

        let now = chrono::Utc::now().timestamp() as f64;
        self.watcher_metrics.last_sync_timestamp.set(now);
        self.watcher_metrics
            .synced_records
            .set(records.len() as f64);

        // Update the active records gauge in the main metrics registry
        let metrics_guard = self.metrics.read().await;
        metrics_guard
            .dns_metrics()
            .set_active_records_count(records.len() as f64);

        Ok(())
    }

    async fn handle_event(&self, event: Event<DnsRecord>) -> Result<(), WatcherError> {
        match event {
            Event::Apply(cr) | Event::InitApply(cr) => {
                let id = cr.metadata.name.as_deref().unwrap_or_default();
                let record = cr.spec.to_dns_record(id)?;
                self.store.put_record(&record)?;
                self.watcher_metrics
                    .events_total
                    .with_label_values(&["apply"])
                    .inc();
                info!(id = id, name = %record.name, "Synced record to LMDB");
                self.patch_status_synced(id).await;
            }
            Event::Delete(cr) => {
                let id = cr.metadata.name.as_deref().unwrap_or_default();
                let deleted = self.store.delete_record_by_id(id)?;
                self.watcher_metrics
                    .events_total
                    .with_label_values(&["delete"])
                    .inc();
                if deleted {
                    info!(id = id, "Deleted record from LMDB");
                } else {
                    warn!(id = id, "Attempted to delete non-existent record from LMDB");
                }
            }
            Event::Init => {
                info!("Watcher stream initialized");
                self.watcher_metrics
                    .events_total
                    .with_label_values(&["init"])
                    .inc();
            }
            Event::InitDone => {
                info!("Watcher initial list complete");
                self.watcher_metrics
                    .events_total
                    .with_label_values(&["init_done"])
                    .inc();
            }
        }
        Ok(())
    }

    async fn patch_status_synced(&self, id: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let status = serde_json::json!({
            "status": DnsRecordStatus {
                synced: true,
                last_synced_at: Some(now),
                error: None,
            }
        });
        let patch = Patch::Merge(&status);
        let pp = PatchParams::default();
        if let Err(e) = self.api.patch_status(id, &pp, &patch).await {
            warn!(id = id, error = %e, "Failed to patch status (non-fatal)");
        }
    }
}
