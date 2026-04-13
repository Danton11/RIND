use chrono::{DateTime, Utc};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::storage::{LmdbStore, StorageError};

#[derive(Debug, thiserror::Error)]
pub enum DatastoreError {
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
    #[error("record not found: {0}")]
    NotFound(String),
}

/// Persistence backend for DNS records. Mutations are per-record so the
/// backing store can commit them atomically; the in-memory `DnsRecords`
/// hashmap is warmed from `load_all_records` at startup and kept coherent
/// by the CRUD handlers.
#[async_trait::async_trait]
pub trait DatastoreProvider: Send + Sync {
    async fn initialize(&self) -> Result<(), DatastoreError>;
    async fn load_all_records(&self) -> Result<DnsRecords, DatastoreError>;
    async fn put_record(&self, record: &DnsRecord) -> Result<(), DatastoreError>;
    async fn delete_record(&self, id: &str) -> Result<(), DatastoreError>;
}

pub struct LmdbDatastoreProvider {
    store: Arc<LmdbStore>,
}

impl LmdbDatastoreProvider {
    pub fn new(store: Arc<LmdbStore>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl DatastoreProvider for LmdbDatastoreProvider {
    async fn initialize(&self) -> Result<(), DatastoreError> {
        Ok(())
    }

    async fn load_all_records(&self) -> Result<DnsRecords, DatastoreError> {
        let mut map = DnsRecords::new();
        for record in self.store.list_all_records()? {
            map.insert(record.id.clone(), record);
        }
        Ok(map)
    }

    async fn put_record(&self, record: &DnsRecord) -> Result<(), DatastoreError> {
        self.store.put_record(record)?;
        Ok(())
    }

    async fn delete_record(&self, id: &str) -> Result<(), DatastoreError> {
        if !self.store.delete_record_by_id(id)? {
            return Err(DatastoreError::NotFound(id.to_string()));
        }
        Ok(())
    }
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
    #[serde(rename = "CNAME")]
    Cname {
        target: String,
    },
    #[serde(rename = "PTR")]
    Ptr {
        target: String,
    },
    #[serde(rename = "NS")]
    Ns {
        target: String,
    },
    #[serde(rename = "MX")]
    Mx {
        preference: u16,
        exchange: String,
    },
    #[serde(rename = "TXT")]
    Txt {
        strings: Vec<String>,
    },
}

impl RecordData {
    /// DNS wire type code (RFC 1035 / 3596).
    pub fn type_code(&self) -> u16 {
        match self {
            RecordData::A { .. } => 1,
            RecordData::Aaaa { .. } => 28,
            RecordData::Cname { .. } => 5,
            RecordData::Ptr { .. } => 12,
            RecordData::Ns { .. } => 2,
            RecordData::Mx { .. } => 15,
            RecordData::Txt { .. } => 16,
        }
    }

    /// Human-readable type name, matches the serde `type` tag.
    pub fn type_name(&self) -> &'static str {
        match self {
            RecordData::A { .. } => "A",
            RecordData::Aaaa { .. } => "AAAA",
            RecordData::Cname { .. } => "CNAME",
            RecordData::Ptr { .. } => "PTR",
            RecordData::Ns { .. } => "NS",
            RecordData::Mx { .. } => "MX",
            RecordData::Txt { .. } => "TXT",
        }
    }

    /// True if this variant is a CNAME. Used by the write path to enforce
    /// RFC 2181 §10.1 — a name holding a CNAME must hold nothing else.
    pub fn is_cname(&self) -> bool {
        matches!(self, RecordData::Cname { .. })
    }

    /// True if multiple records are legal at the same `(name, type)`.
    ///
    /// Singleton types (A, AAAA, CNAME, PTR) reject any second record at the
    /// same `(name, type)` at write time. Multi-value types (NS, MX, TXT)
    /// instead reject only exact-rdata duplicates — RFC 2181 §5 says an RRSet
    /// is a set, not a bag. Different rdata, same `(name, type)` is how
    /// delegation sets, MX fallbacks, and TXT fan-out work.
    ///
    /// A/AAAA staying singleton is a *policy* choice, not an RFC requirement
    /// — real DNS allows multi-A for round-robin. Flip the match arm if you
    /// want that later.
    pub fn allows_multiple(&self) -> bool {
        match self {
            RecordData::A { .. }
            | RecordData::Aaaa { .. }
            | RecordData::Cname { .. }
            | RecordData::Ptr { .. } => false,
            RecordData::Ns { .. } | RecordData::Mx { .. } | RecordData::Txt { .. } => true,
        }
    }

    /// Rdata-level invariants that aren't enforced by the type system alone.
    /// Called from `DnsRecord::validate`. Returns the first violation found.
    pub fn validate_rdata(&self) -> Result<(), ValidationError> {
        match self {
            RecordData::Txt { strings } => {
                if strings.is_empty() {
                    return Err(ValidationError::MissingField(
                        "TXT record must have at least one string".to_string(),
                    ));
                }
                // RFC 1035 §3.3.14: each character-string is 1 length octet
                // plus up to 255 bytes. Users with longer values must split
                // into multiple Vec entries themselves — we don't auto-split
                // because that would silently mutate user input.
                for (i, s) in strings.iter().enumerate() {
                    if s.len() > 255 {
                        return Err(ValidationError::InvalidDomainName(format!(
                            "TXT string #{} is {} bytes; RFC 1035 §3.3.14 limit is 255",
                            i,
                            s.len()
                        )));
                    }
                }
                Ok(())
            }
            _ => Ok(()),
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

#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("Record not found: {0}")]
    NotFound(String),
    #[error("Validation error: {0}")]
    ValidationError(#[from] ValidationError),
    #[error("Duplicate record name: {0}")]
    DuplicateRecord(String),
    #[error("Datastore error: {0}")]
    DatastoreError(#[from] DatastoreError),
}

impl RecordError {
    pub fn to_status_code(&self) -> u16 {
        match self {
            RecordError::NotFound(_) => 404,
            RecordError::ValidationError(_) => 400,
            RecordError::DuplicateRecord(_) => 409,
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

        self.data.validate_rdata()?;

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

/// Outcome of the shared conflict check between `create_record` and
/// `update_record`. Carries both the metric-label reason and the caller-
/// facing error so both call sites stay in sync.
enum RrsetConflict {
    CnameExclusivity,
    SingletonDuplicate,
    RrsetDuplicate,
}

impl RrsetConflict {
    fn reason_label(&self) -> &'static str {
        match self {
            RrsetConflict::CnameExclusivity => "cname_conflict",
            RrsetConflict::SingletonDuplicate => "duplicate_record",
            RrsetConflict::RrsetDuplicate => "rrset_duplicate",
        }
    }

    fn into_error(self, candidate: &DnsRecord) -> RecordError {
        match self {
            RrsetConflict::CnameExclusivity => RecordError::DuplicateRecord(format!(
                "CNAME at '{}' conflicts with existing record (RFC 2181 §10.1)",
                candidate.name
            )),
            RrsetConflict::SingletonDuplicate => RecordError::DuplicateRecord(format!(
                "Record with name '{}' and type '{}' already exists",
                candidate.name,
                candidate.data.type_name()
            )),
            RrsetConflict::RrsetDuplicate => RecordError::DuplicateRecord(format!(
                "RRSet for '{}' {} already contains this rdata (RFC 2181 §5)",
                candidate.name,
                candidate.data.type_name()
            )),
        }
    }
}

/// Check a candidate record against the live set for RFC 2181 conflicts.
///
/// Three rules fire, in order:
///   1. §10.1 CNAME exclusivity: a name holding a CNAME can hold nothing else.
///   2. Singleton types (A, AAAA, CNAME, PTR) reject any second record with
///      the same `(name, type)`.
///   3. Multi-value types (NS, MX, TXT) reject only exact-rdata duplicates —
///      §5 says an RRSet is a set, not a bag.
///
/// `exclude_id` skips the given record — used by `update_record` so a record
/// does not conflict with its own pre-update state.
fn check_rrset_conflict(
    existing: &DnsRecords,
    candidate: &DnsRecord,
    exclude_id: Option<&str>,
) -> Result<(), RrsetConflict> {
    for (other_id, other) in existing.iter() {
        if Some(other_id.as_str()) == exclude_id {
            continue;
        }
        if other.name != candidate.name {
            continue;
        }
        if other.data.is_cname() || candidate.data.is_cname() {
            return Err(RrsetConflict::CnameExclusivity);
        }
        if other.data.type_name() != candidate.data.type_name() {
            continue;
        }
        if !candidate.data.allows_multiple() {
            return Err(RrsetConflict::SingletonDuplicate);
        }
        if other.data == candidate.data {
            return Err(RrsetConflict::RrsetDuplicate);
        }
    }
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

    if let Err(conflict) = check_rrset_conflict(&records_guard, &new_record, None) {
        let label = conflict.reason_label();
        let error = conflict.into_error(&new_record);
        record_failure(&metrics_registry, "create", label, start_time).await;
        return Err(error);
    }

    if let Err(e) = datastore.put_record(&new_record).await {
        record_failure(&metrics_registry, "create", "io_error", start_time).await;
        return Err(RecordError::DatastoreError(e));
    }

    let record_id = new_record.id.clone();
    records_guard.insert(record_id.clone(), new_record.clone());

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

    if let Err(conflict) = check_rrset_conflict(&records_guard, &existing_record, Some(id)) {
        let label = conflict.reason_label();
        let error = conflict.into_error(&existing_record);
        record_failure(&metrics_registry, "update", label, start_time).await;
        return Err(error);
    }

    existing_record.touch();

    if let Err(e) = datastore.put_record(&existing_record).await {
        record_failure(&metrics_registry, "update", "io_error", start_time).await;
        return Err(RecordError::DatastoreError(e));
    }

    records_guard.insert(id.to_string(), existing_record.clone());

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

            if let Err(e) = datastore.delete_record(id).await {
                record_failure(&metrics_registry, "delete", "io_error", start_time).await;
                return Err(RecordError::DatastoreError(e));
            }

            records_guard.remove(id);

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
