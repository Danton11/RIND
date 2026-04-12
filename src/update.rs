use chrono::{DateTime, Utc};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Error as IoError, Write};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Trait for datastore operations — pluggable backend for record persistence.
///
/// Current shape is save-all / load-all. A future transactional backend would
/// add per-record methods (`put_record`, `delete_record`) as an additive change
/// so it doesn't have to serialize the whole HashMap on every mutation.
#[async_trait::async_trait]
pub trait DatastoreProvider: Send + Sync {
    /// Initialize the datastore (create file, connect to DB, etc.)
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Check if the datastore is properly configured and accessible
    #[allow(dead_code)]
    async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>;

    /// Load all records from the datastore
    async fn load_all_records(
        &self,
    ) -> Result<DnsRecords, Box<dyn std::error::Error + Send + Sync>>;

    /// Save all records to the datastore
    async fn save_all_records(
        &self,
        records: &DnsRecords,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Type-specific record payload. Each variant carries exactly the fields
/// that record type needs — invalid combinations are unrepresentable.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "UPPERCASE")]
pub enum RecordData {
    A {
        ip: Ipv4Addr,
    },
    #[serde(rename = "AAAA")]
    Aaaa {
        ip: Ipv6Addr,
    },
}

impl RecordData {
    /// DNS wire type code (RFC 1035 / 3596).
    pub fn type_code(&self) -> u16 {
        match self {
            RecordData::A { .. } => 1,
            RecordData::Aaaa { .. } => 28,
        }
    }

    /// Human-readable type name, matches the serde `type` tag.
    pub fn type_name(&self) -> &'static str {
        match self {
            RecordData::A { .. } => "A",
            RecordData::Aaaa { .. } => "AAAA",
        }
    }
}

/// A DNS record — metadata plus a type-specific data payload.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct DnsRecord {
    pub id: String,
    pub name: String,
    pub ttl: u32,
    pub class: String,
    #[serde(flatten)]
    pub data: RecordData,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub type DnsRecords = HashMap<String, DnsRecord>;

/// Generate a new UUID for a DNS record
pub fn generate_record_id() -> String {
    Uuid::new_v4().to_string()
}

/// Validation error types
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Invalid domain name: {0}")]
    InvalidDomainName(String),
    #[error("Invalid TTL value: {0}")]
    InvalidTtl(String),
    #[error("Invalid class: {0}")]
    InvalidClass(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Record management error types
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("Record not found: {0}")]
    NotFound(String),
    #[error("Validation error: {0}")]
    ValidationError(#[from] ValidationError),
    #[error("Duplicate record name: {0}")]
    DuplicateRecord(String),
    #[error("IO error: {0}")]
    IoError(#[from] IoError),
    #[error("Datastore error: {0}")]
    DatastoreError(String),
}

impl RecordError {
    /// Convert RecordError to HTTP status code
    pub fn to_status_code(&self) -> u16 {
        match self {
            RecordError::NotFound(_) => 404,
            RecordError::ValidationError(_) => 400,
            RecordError::DuplicateRecord(_) => 409,
            RecordError::IoError(_) => 500,
            RecordError::DatastoreError(_) => 500,
        }
    }
}

/// Generic API response wrapper for consistent JSON responses
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: String,
}

impl<T> ApiResponse<T> {
    /// Create a successful response with data
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Create an error response
    pub fn error(error_message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error_message),
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

/// Response structure for listing records with pagination
#[derive(Debug, Serialize, Deserialize)]
pub struct RecordListResponse {
    pub records: Vec<DnsRecord>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
}

/// Request structure for creating new records.
/// `data` is flattened — the JSON body looks like
/// `{"name": "...", "ttl": 300, "type": "A", "ip": "1.2.3.4"}`.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRecordRequest {
    pub name: String,
    pub ttl: Option<u32>,
    pub class: Option<String>,
    #[serde(flatten)]
    pub data: RecordData,
}

/// Request structure for partial record updates.
/// `data` is NOT flattened — omit it to leave the record's type/payload
/// unchanged. Include it (e.g. `{"data": {"type": "AAAA", "ip": "::1"}}`)
/// to replace the payload wholesale.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct UpdateRecordRequest {
    pub name: Option<String>,
    pub ttl: Option<u32>,
    pub class: Option<String>,
    pub data: Option<RecordData>,
}

impl DnsRecord {
    /// Create a new DNS record with generated UUID and current timestamps
    pub fn new(name: String, ttl: u32, class: String, data: RecordData) -> Self {
        let now = Utc::now();
        Self {
            id: generate_record_id(),
            name,
            ttl,
            class,
            data,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the record's updated_at timestamp
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Validate the DNS record data integrity.
    /// Most invariants (record type, payload shape) are enforced by the
    /// `RecordData` enum at construction time — this only covers things
    /// the type system can't express.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.name.is_empty() {
            return Err(ValidationError::MissingField("name".to_string()));
        }

        if !self
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return Err(ValidationError::InvalidDomainName(self.name.clone()));
        }

        // Max 7 days TTL
        if self.ttl > 86400 * 7 {
            return Err(ValidationError::InvalidTtl(format!(
                "TTL too large: {}",
                self.ttl
            )));
        }

        let valid_classes = ["IN", "CH", "HS"];
        if !valid_classes.contains(&self.class.as_str()) {
            return Err(ValidationError::InvalidClass(self.class.clone()));
        }

        Ok(())
    }

    /// Check if this record has the same content as another (ignoring timestamps and ID)
    #[allow(dead_code)]
    pub fn has_same_content(&self, other: &DnsRecord) -> bool {
        self.name == other.name
            && self.ttl == other.ttl
            && self.class == other.class
            && self.data == other.data
    }
}

// ---------- Datastore: JSON Lines file provider ----------

/// File-based datastore using JSON Lines — one serialized `DnsRecord` per line.
///
/// This is the default provider. Additional providers can slot in behind
/// `DatastoreProvider`, which is the seam that keeps the rest of the code
/// backend-agnostic.
pub struct JsonlFileDatastoreProvider {
    file_path: String,
}

impl JsonlFileDatastoreProvider {
    pub fn new(file_path: String) -> Self {
        Self { file_path }
    }
}

#[async_trait::async_trait]
impl DatastoreProvider for JsonlFileDatastoreProvider {
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Create the file if it doesn't exist, leave it alone otherwise.
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;
        info!("Datastore initialized at {}", self.file_path);
        Ok(())
    }

    async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(std::path::Path::new(&self.file_path).exists())
    }

    async fn load_all_records(
        &self,
    ) -> Result<DnsRecords, Box<dyn std::error::Error + Send + Sync>> {
        load_records_from_file(&self.file_path)
    }

    async fn save_all_records(
        &self,
        records: &DnsRecords,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        save_records_to_file(&self.file_path, records).map_err(|e| e.into())
    }
}

/// Load records from a JSON Lines file. Blank lines and `#`-comments are skipped.
pub fn load_records_from_file(
    file_path: &str,
) -> Result<DnsRecords, Box<dyn std::error::Error + Send + Sync>> {
    let mut records = DnsRecords::new();

    let file = match File::open(file_path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(records),
        Err(e) => return Err(Box::new(e)),
    };

    let reader = BufReader::new(file);
    for (lineno, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match serde_json::from_str::<DnsRecord>(trimmed) {
            Ok(record) => {
                records.insert(record.id.clone(), record);
            }
            Err(e) => {
                error!(
                    "Failed to parse record at {}:{}: {} (line: {})",
                    file_path,
                    lineno + 1,
                    e,
                    trimmed
                );
            }
        }
    }
    Ok(records)
}

/// Save records to a JSON Lines file (one record per line).
pub fn save_records_to_file(file_path: &str, records: &DnsRecords) -> Result<(), IoError> {
    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);

    writeln!(
        writer,
        "# RIND DNS records — JSON Lines (one record per line)"
    )?;

    for record in records.values() {
        let json = serde_json::to_string(record).map_err(|e| {
            IoError::new(
                std::io::ErrorKind::InvalidData,
                format!("serialize error: {}", e),
            )
        })?;
        writeln!(writer, "{}", json)?;
    }
    writer.flush()?;
    Ok(())
}

// ---------- CRUD ----------

/// Create a new DNS record.
pub async fn create_record(
    records: Arc<RwLock<DnsRecords>>,
    datastore: Arc<dyn DatastoreProvider>,
    name: String,
    ttl: u32,
    class: String,
    data: RecordData,
    metrics_registry: Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
) -> Result<DnsRecord, RecordError> {
    let start_time = std::time::Instant::now();
    let new_record = DnsRecord::new(name.clone(), ttl, class, data);

    if let Err(validation_error) = new_record.validate() {
        record_failure(&metrics_registry, "create", "validation_error", start_time).await;
        return Err(RecordError::ValidationError(validation_error));
    }

    let mut records_guard = records.write().await;

    // Duplicate check: same name + same record type (e.g. two "A" records for foo.com).
    for existing_record in records_guard.values() {
        if existing_record.name == new_record.name
            && existing_record.data.type_name() == new_record.data.type_name()
        {
            let error = RecordError::DuplicateRecord(format!(
                "Record with name '{}' and type '{}' already exists",
                new_record.name,
                new_record.data.type_name()
            ));
            record_failure(&metrics_registry, "create", "duplicate_record", start_time).await;
            return Err(error);
        }
    }

    let record_id = new_record.id.clone();
    records_guard.insert(record_id.clone(), new_record.clone());

    if let Err(e) = datastore.save_all_records(&records_guard).await {
        record_failure(&metrics_registry, "create", "io_error", start_time).await;
        return Err(RecordError::DatastoreError(e.to_string()));
    }

    info!("Created new record with ID {}: {}", record_id, name);
    debug!("New record details: {:?}", new_record);

    record_success(
        &metrics_registry,
        "create",
        start_time,
        Some(records_guard.len()),
    )
    .await;
    Ok(new_record)
}

/// Create a new DNS record from a `CreateRecordRequest`.
pub async fn create_record_from_request(
    records: Arc<RwLock<DnsRecords>>,
    datastore: Arc<dyn DatastoreProvider>,
    create_request: CreateRecordRequest,
    metrics_registry: Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
) -> Result<DnsRecord, RecordError> {
    let ttl = create_request.ttl.unwrap_or(300);
    let class = create_request.class.unwrap_or_else(|| "IN".to_string());

    create_record(
        records,
        datastore,
        create_request.name,
        ttl,
        class,
        create_request.data,
        metrics_registry,
    )
    .await
}

/// Retrieve a specific DNS record by its UUID
pub async fn get_record(
    records: Arc<RwLock<DnsRecords>>,
    id: &str,
    metrics_registry: Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
) -> Result<DnsRecord, RecordError> {
    let start_time = std::time::Instant::now();
    let records_guard = records.read().await;

    match records_guard.get(id) {
        Some(record) => {
            debug!("Retrieved record with ID {}: {}", id, record.name);
            record_success(&metrics_registry, "read", start_time, None).await;
            Ok(record.clone())
        }
        None => {
            debug!("Record not found with ID: {}", id);
            record_failure(&metrics_registry, "read", "not_found", start_time).await;
            Err(RecordError::NotFound(format!(
                "Record with ID '{}' not found",
                id
            )))
        }
    }
}

/// List DNS records with pagination support
pub async fn list_records(
    records: Arc<RwLock<DnsRecords>>,
    page: usize,
    per_page: usize,
    metrics_registry: Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
) -> Result<RecordListResponse, RecordError> {
    let start_time = std::time::Instant::now();
    let records_guard = records.read().await;

    if page == 0 {
        record_failure(&metrics_registry, "list", "validation_error", start_time).await;
        return Err(RecordError::ValidationError(ValidationError::InvalidTtl(
            "Page number must be greater than 0".to_string(),
        )));
    }

    if per_page == 0 || per_page > 1000 {
        record_failure(&metrics_registry, "list", "validation_error", start_time).await;
        return Err(RecordError::ValidationError(ValidationError::InvalidTtl(
            "Per page must be between 1 and 1000".to_string(),
        )));
    }

    let total = records_guard.len();
    let start_index = (page - 1) * per_page;

    let mut all_records: Vec<DnsRecord> = records_guard.values().cloned().collect();
    all_records.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let paginated_records: Vec<DnsRecord> = all_records
        .into_iter()
        .skip(start_index)
        .take(per_page)
        .collect();

    debug!(
        "Listed {} records (page {}, per_page {})",
        paginated_records.len(),
        page,
        per_page
    );

    record_success(&metrics_registry, "list", start_time, None).await;

    Ok(RecordListResponse {
        records: paginated_records,
        total,
        page,
        per_page,
    })
}

/// Update an existing DNS record with partial-update semantics.
///
/// `name`, `ttl`, `class` are partial — omit to leave unchanged.
/// `data` is all-or-nothing — include to replace the whole payload
/// (including record type), omit to leave the payload unchanged.
pub async fn update_record(
    records: Arc<RwLock<DnsRecords>>,
    datastore: Arc<dyn DatastoreProvider>,
    id: &str,
    update_request: UpdateRecordRequest,
    metrics_registry: Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
) -> Result<DnsRecord, RecordError> {
    let start_time = std::time::Instant::now();
    let mut records_guard = records.write().await;

    let mut existing_record = match records_guard.get(id) {
        Some(record) => record.clone(),
        None => {
            record_failure(&metrics_registry, "update", "not_found", start_time).await;
            return Err(RecordError::NotFound(format!(
                "Record with ID '{}' not found",
                id
            )));
        }
    };

    if let Some(name) = update_request.name {
        existing_record.name = name;
    }
    if let Some(ttl) = update_request.ttl {
        existing_record.ttl = ttl;
    }
    if let Some(class) = update_request.class {
        existing_record.class = class;
    }
    if let Some(data) = update_request.data {
        existing_record.data = data;
    }

    if let Err(validation_error) = existing_record.validate() {
        record_failure(&metrics_registry, "update", "validation_error", start_time).await;
        return Err(RecordError::ValidationError(validation_error));
    }

    // Duplicate check (excluding the current record)
    for (other_id, other_record) in records_guard.iter() {
        if other_id != id
            && other_record.name == existing_record.name
            && other_record.data.type_name() == existing_record.data.type_name()
        {
            record_failure(&metrics_registry, "update", "duplicate_record", start_time).await;
            return Err(RecordError::DuplicateRecord(format!(
                "Record with name '{}' and type '{}' already exists",
                existing_record.name,
                existing_record.data.type_name()
            )));
        }
    }

    existing_record.touch();
    records_guard.insert(id.to_string(), existing_record.clone());

    if let Err(e) = datastore.save_all_records(&records_guard).await {
        record_failure(&metrics_registry, "update", "io_error", start_time).await;
        return Err(RecordError::DatastoreError(e.to_string()));
    }

    info!("Updated record with ID {}: {}", id, existing_record.name);
    record_success(
        &metrics_registry,
        "update",
        start_time,
        Some(records_guard.len()),
    )
    .await;

    Ok(existing_record)
}

/// Delete a DNS record by its UUID
pub async fn delete_record(
    records: Arc<RwLock<DnsRecords>>,
    datastore: Arc<dyn DatastoreProvider>,
    id: &str,
    metrics_registry: Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
) -> Result<(), RecordError> {
    let start_time = std::time::Instant::now();
    let mut records_guard = records.write().await;

    match records_guard.get(id) {
        Some(record) => {
            let record_name = record.name.clone();
            records_guard.remove(id);

            if let Err(e) = datastore.save_all_records(&records_guard).await {
                record_failure(&metrics_registry, "delete", "io_error", start_time).await;
                return Err(RecordError::DatastoreError(e.to_string()));
            }

            info!("Deleted record with ID {}: {}", id, record_name);
            record_success(
                &metrics_registry,
                "delete",
                start_time,
                Some(records_guard.len()),
            )
            .await;
            Ok(())
        }
        None => {
            debug!("Attempted to delete non-existent record with ID: {}", id);
            record_failure(&metrics_registry, "delete", "not_found", start_time).await;
            Err(RecordError::NotFound(format!(
                "Record with ID '{}' not found",
                id
            )))
        }
    }
}

// ---------- metrics helpers ----------

async fn record_success(
    metrics_registry: &Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
    op: &str,
    start_time: std::time::Instant,
    active_count: Option<usize>,
) {
    if let Some(registry) = metrics_registry {
        let duration = start_time.elapsed().as_secs_f64();
        let registry_guard = registry.read().await;
        registry_guard
            .dns_metrics()
            .record_operation_success(op, duration);
        if let Some(n) = active_count {
            registry_guard
                .dns_metrics()
                .set_active_records_count(n as f64);
        }
    }
}

async fn record_failure(
    metrics_registry: &Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
    op: &str,
    reason: &str,
    start_time: std::time::Instant,
) {
    if let Some(registry) = metrics_registry {
        let duration = start_time.elapsed().as_secs_f64();
        let registry_guard = registry.read().await;
        registry_guard
            .dns_metrics()
            .record_operation_failure(op, reason, duration);
    }
}
