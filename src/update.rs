use std::collections::HashMap;
use std::sync::Arc;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write, BufRead, Error as IoError};
use tokio::sync::RwLock;
use std::net::Ipv4Addr;
use serde::{Deserialize, Serialize};
use log::{info, debug, error};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Trait for datastore operations - allows easy switching between file and database storage
#[async_trait::async_trait]
pub trait DatastoreProvider: Send + Sync {
    /// Initialize the datastore (create file, connect to DB, etc.)
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    
    /// Check if the datastore is properly configured and accessible
    async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>;
    
    /// Load all records from the datastore
    async fn load_all_records(&self) -> Result<DnsRecords, Box<dyn std::error::Error + Send + Sync>>;
    
    /// Save all records to the datastore
    async fn save_all_records(&self, records: &DnsRecords) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DnsRecord {
    pub id: String,                    // UUID identifier
    pub name: String,
    pub ip: Option<Ipv4Addr>,         // Make IP optional to handle non-IP records
    pub ttl: u32,
    pub record_type: String,
    pub class: String,
    pub value: Option<String>,        // Additional field to handle non-IP values like CNAME or TXT
    pub created_at: DateTime<Utc>,    // Creation timestamp
    pub updated_at: DateTime<Utc>,    // Last update timestamp
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
    #[error("Invalid IP address: {0}")]
    InvalidIpAddress(String),
    #[error("Invalid TTL value: {0}")]
    InvalidTtl(String),
    #[error("Invalid record type: {0}")]
    InvalidRecordType(String),
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
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

impl RecordError {
    /// Convert RecordError to HTTP status code
    pub fn to_status_code(&self) -> u16 {
        match self {
            RecordError::NotFound(_) => 404,
            RecordError::ValidationError(_) => 400,
            RecordError::DuplicateRecord(_) => 409,
            RecordError::IoError(_) => 500,
            RecordError::SerializationError(_) => 500,
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

/// Request structure for creating new records
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRecordRequest {
    pub name: String,
    pub ip: Option<String>,
    pub ttl: Option<u32>,
    pub record_type: Option<String>,
    pub class: Option<String>,
    pub value: Option<String>,
}

/// Request structure for partial record updates
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateRecordRequest {
    pub name: Option<String>,
    pub ip: Option<String>,
    pub ttl: Option<u32>,
    pub record_type: Option<String>,
    pub class: Option<String>,
    pub value: Option<String>,
}

impl DnsRecord {
    /// Create a new DNS record with generated UUID and current timestamps
    pub fn new(
        name: String,
        ip: Option<Ipv4Addr>,
        ttl: u32,
        record_type: String,
        class: String,
        value: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: generate_record_id(),
            name,
            ip,
            ttl,
            record_type,
            class,
            value,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the record's updated_at timestamp
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Validate the DNS record data integrity
    pub fn validate(&self) -> Result<(), ValidationError> {
        // Validate domain name
        if self.name.is_empty() {
            return Err(ValidationError::MissingField("name".to_string()));
        }
        
        // Basic domain name validation (simplified)
        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_') {
            return Err(ValidationError::InvalidDomainName(self.name.clone()));
        }

        // Validate TTL (should be reasonable range)
        if self.ttl > 86400 * 7 { // Max 7 days
            return Err(ValidationError::InvalidTtl(format!("TTL too large: {}", self.ttl)));
        }

        // Validate record type
        let valid_types = ["A", "AAAA", "CNAME", "TXT", "MX", "NS", "PTR", "SOA", "SRV"];
        if !valid_types.contains(&self.record_type.as_str()) {
            return Err(ValidationError::InvalidRecordType(self.record_type.clone()));
        }

        // Validate class
        let valid_classes = ["IN", "CH", "HS"];
        if !valid_classes.contains(&self.class.as_str()) {
            return Err(ValidationError::InvalidClass(self.class.clone()));
        }

        // Validate A record has IP address
        if self.record_type == "A" && self.ip.is_none() {
            return Err(ValidationError::MissingField("ip for A record".to_string()));
        }

        // Validate CNAME/TXT records have value
        if (self.record_type == "CNAME" || self.record_type == "TXT") && self.value.is_none() {
            return Err(ValidationError::MissingField(format!("value for {} record", self.record_type)));
        }

        Ok(())
    }

    /// Check if this record has the same content as another (ignoring timestamps and ID)
    pub fn has_same_content(&self, other: &DnsRecord) -> bool {
        self.name == other.name
            && self.ip == other.ip
            && self.ttl == other.ttl
            && self.record_type == other.record_type
            && self.class == other.class
            && self.value == other.value
    }
}

pub fn load_records_from_file(file_path: &str) -> Result<DnsRecords, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut records = DnsRecords::new();

    for line in reader.lines() {
        let line = line?.trim().to_string();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        
        let parts: Vec<&str> = line.split(':').collect();
        
        // Check if this is the new UUID-based format: id:name:ip:ttl:type:class
        if parts.len() >= 6 && Uuid::parse_str(parts[0]).is_ok() {
            let id = parts[0].to_string();
            let name = parts[1].to_string();
            let ip = parts[2].parse::<Ipv4Addr>().ok();
            let ttl = parts[3].parse::<u32>().map_err(|e| {
                error!("Failed to parse TTL for line {}: {}", line, e);
                e
            })?;
            let record_type = parts[4].to_string();
            let class = parts[5].to_string();
            let value = if parts.len() > 6 { Some(parts[6].to_string()) } else { None };
            
            // For new format, we'll use current time as both created_at and updated_at
            // since we don't have historical timestamp data
            let now = Utc::now();
            let record = DnsRecord {
                id,
                name: name.clone(),
                ip,
                ttl,
                record_type,
                class,
                value,
                created_at: now,
                updated_at: now,
            };
            records.insert(record.id.clone(), record);
        }
        // Legacy format handling - generate UUIDs for existing records
        else if parts.len() >= 5 {
            let ip = parts[1].parse::<Ipv4Addr>().ok();
            let ttl = parts[2].parse::<u32>().map_err(|e| {
                error!("Failed to parse TTL for line {}: {}", line, e);
                e
            })?;
            let now = Utc::now();
            let record = DnsRecord {
                id: generate_record_id(),
                name: parts[0].to_string(),
                ip,
                ttl,
                record_type: parts[3].to_string(),
                class: parts[4].to_string(),
                value: None,
                created_at: now,
                updated_at: now,
            };
            records.insert(record.id.clone(), record);
        }
        // Legacy CNAME record
        else if parts.len() == 4 && parts[0].starts_with('C') {
            let now = Utc::now();
            let record = DnsRecord {
                id: generate_record_id(),
                name: parts[0].to_string(),
                ip: None,
                ttl: parts[2].parse::<u32>()?,
                record_type: "CNAME".to_string(),
                class: parts[3].to_string(),
                value: Some(parts[1].to_string()),
                created_at: now,
                updated_at: now,
            };
            records.insert(record.id.clone(), record);
        }
        // Legacy TXT record
        else if parts.len() == 5 && parts[0].starts_with('\'') {
            let now = Utc::now();
            let record = DnsRecord {
                id: generate_record_id(),
                name: parts[0].to_string(),
                ip: None,
                ttl: parts[2].parse::<u32>()?,
                record_type: "TXT".to_string(),
                class: parts[4].to_string(),
                value: Some(parts[1].to_string()),
                created_at: now,
                updated_at: now,
            };
            records.insert(record.id.clone(), record);
        } else {
            error!("Invalid record format: {}", line);
        }
    }
    Ok(records)
}

pub fn save_records_to_file(file_path: &str, records: &DnsRecords) -> Result<(), IoError> {
    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);

    // Write header comment explaining the new format
    writeln!(writer, "# DNS Records File - Format: id:name:ip:ttl:type:class:value")?;
    writeln!(writer, "# id: UUID identifier for the record")?;
    writeln!(writer, "# name: domain name")?;
    writeln!(writer, "# ip: IP address (for A records) or empty for other types")?;
    writeln!(writer, "# ttl: time to live in seconds")?;
    writeln!(writer, "# type: record type (A, CNAME, TXT, etc.)")?;
    writeln!(writer, "# class: record class (usually IN)")?;
    writeln!(writer, "# value: additional value for CNAME, TXT records (optional)")?;
    writeln!(writer)?;

    for record in records.values() {
        let ip_str = record.ip
            .map(|ip| ip.to_string())
            .unwrap_or_default();
        
        let value_str = record.value.as_ref()
            .map(|v| format!(":{}", v))
            .unwrap_or_default();

        writeln!(writer, "{}:{}:{}:{}:{}:{}{}", 
            record.id, record.name, ip_str, record.ttl, 
            record.record_type, record.class, value_str)?;
    }
    writer.flush()?;
    Ok(())
}

pub fn load_records(file_path: &str) -> Arc<RwLock<DnsRecords>> {
    match load_records_from_file(file_path) {
        Ok(records) => Arc::new(RwLock::new(records)),
        Err(e) => {
            log::error!("Failed to load DNS records: {}", e);
            Arc::new(RwLock::new(DnsRecords::new()))
        }
    }
}

/// Initialize an empty DNS records file with the new UUID format
/// Creates the file with proper header comments explaining the format structure
pub fn initialize_empty_datastore(file_path: &str) -> Result<(), IoError> {
    info!("Initializing empty datastore at: {}", file_path);
    
    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);

    // Write comprehensive header comment explaining the new format structure
    writeln!(writer, "# DNS Records File - Enhanced UUID Format")?;
    writeln!(writer, "# ============================================")?;
    writeln!(writer, "#")?;
    writeln!(writer, "# Format: id:name:ip:ttl:type:class:value")?;
    writeln!(writer, "#")?;
    writeln!(writer, "# Field Descriptions:")?;
    writeln!(writer, "# - id: UUID v4 identifier for the record (unique)")?;
    writeln!(writer, "# - name: domain name (e.g., example.com)")?;
    writeln!(writer, "# - ip: IP address for A records, empty for other types")?;
    writeln!(writer, "# - ttl: time to live in seconds (e.g., 300)")?;
    writeln!(writer, "# - type: record type (A, AAAA, CNAME, TXT, MX, NS, PTR, SOA, SRV)")?;
    writeln!(writer, "# - class: record class (IN, CH, HS - usually IN)")?;
    writeln!(writer, "# - value: additional value for CNAME, TXT records (optional)")?;
    writeln!(writer, "#")?;
    writeln!(writer, "# Examples:")?;
    writeln!(writer, "# 550e8400-e29b-41d4-a716-446655440000:example.com:93.184.216.34:300:A:IN")?;
    writeln!(writer, "# 6ba7b810-9dad-11d1-80b4-00c04fd430c8:www.example.com::300:CNAME:IN:example.com")?;
    writeln!(writer, "# 6ba7b811-9dad-11d1-80b4-00c04fd430c8:example.com::300:TXT:IN:v=spf1 include:_spf.google.com ~all")?;
    writeln!(writer, "#")?;
    writeln!(writer, "# This file was created with the enhanced record management system")?;
    writeln!(writer, "# Creation time: {}", Utc::now().format("%Y-%m-%d %H:%M:%S UTC"))?;
    writeln!(writer, "#")?;
    writeln!(writer)?;

    writer.flush()?;
    
    info!("Successfully initialized empty datastore with UUID format header");
    Ok(())
}

/// Check if a datastore file exists and is properly formatted
pub fn validate_datastore_format(file_path: &str) -> Result<bool, IoError> {
    match File::open(file_path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let mut has_header = false;
            let mut has_uuid_format = false;
            
            for line in reader.lines() {
                let line = line?;
                let trimmed = line.trim();
                
                // Check for our header format
                if trimmed.contains("DNS Records File - Enhanced UUID Format") {
                    has_header = true;
                }
                
                // Skip comments and empty lines
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                
                // Check if we have UUID format records
                let parts: Vec<&str> = trimmed.split(':').collect();
                if parts.len() >= 6 && Uuid::parse_str(parts[0]).is_ok() {
                    has_uuid_format = true;
                    break;
                }
            }
            
            Ok(has_header || has_uuid_format)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File doesn't exist, which is fine - we can initialize it
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// File-based datastore provider implementation
pub struct FileDatastoreProvider {
    file_path: String,
}

impl FileDatastoreProvider {
    pub fn new(file_path: String) -> Self {
        Self { file_path }
    }
}

#[async_trait::async_trait]
impl DatastoreProvider for FileDatastoreProvider {
    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        ensure_datastore_initialized(&self.file_path)
    }
    
    async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        validate_datastore_format(&self.file_path).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
    
    async fn load_all_records(&self) -> Result<DnsRecords, Box<dyn std::error::Error + Send + Sync>> {
        load_records_from_file(&self.file_path)
    }
    
    async fn save_all_records(&self, records: &DnsRecords) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        save_records_to_file(&self.file_path, records).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

/// Initialize datastore if it doesn't exist or is in legacy format
pub fn ensure_datastore_initialized(file_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match validate_datastore_format(file_path) {
        Ok(true) => {
            debug!("Datastore already exists with proper UUID format: {}", file_path);
            Ok(())
        }
        Ok(false) => {
            info!("Datastore not found or in legacy format, initializing: {}", file_path);
            initialize_empty_datastore(file_path)?;
            Ok(())
        }
        Err(e) => {
            error!("Error validating datastore format: {}", e);
            Err(Box::new(e))
        }
    }
}

/// Create a datastore provider based on configuration
/// In the future, this can be extended to return database providers
pub fn create_datastore_provider(file_path: &str) -> Box<dyn DatastoreProvider> {
    // For now, always return file provider
    // In the future, this could check environment variables or config to determine provider type
    Box::new(FileDatastoreProvider::new(file_path.to_string()))
}

/// Create a new DNS record with UUID generation and validation
/// Validates for duplicate names and invalid data, then persists immediately
pub async fn create_record(
    records: Arc<RwLock<DnsRecords>>, 
    name: String,
    ip: Option<Ipv4Addr>,
    ttl: u32,
    record_type: String,
    class: String,
    value: Option<String>,
    metrics_registry: Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
) -> Result<DnsRecord, RecordError> {
    let start_time = std::time::Instant::now();
    // Create new record with generated UUID and timestamps
    let new_record = DnsRecord::new(name.clone(), ip, ttl, record_type, class, value);
    
    // Validate the record data
    if let Err(validation_error) = new_record.validate() {
        // Record metrics for failed operation
        if let Some(registry) = &metrics_registry {
            let duration = start_time.elapsed().as_secs_f64();
            let registry_guard = registry.read().await;
            registry_guard.dns_metrics().record_operation_failure("create", "validation_error", duration);
        }
        return Err(RecordError::ValidationError(validation_error));
    }
    
    let mut records_guard = records.write().await;
    
    // Check for duplicate names (same name with same type)
    for existing_record in records_guard.values() {
        if existing_record.name == new_record.name && existing_record.record_type == new_record.record_type {
            let error = RecordError::DuplicateRecord(format!(
                "Record with name '{}' and type '{}' already exists", 
                new_record.name, 
                new_record.record_type
            ));
            
            // Record metrics for failed operation
            if let Some(registry) = &metrics_registry {
                let duration = start_time.elapsed().as_secs_f64();
                let registry_guard = registry.read().await;
                registry_guard.dns_metrics().record_operation_failure("create", "duplicate_record", duration);
            }
            
            return Err(error);
        }
    }
    
    // Insert the new record
    let record_id = new_record.id.clone();
    records_guard.insert(record_id.clone(), new_record.clone());
    
    // Persist to file immediately
    if let Err(io_error) = save_records_to_file("dns_records.txt", &*records_guard) {
        // Record metrics for failed operation
        if let Some(registry) = &metrics_registry {
            let duration = start_time.elapsed().as_secs_f64();
            let registry_guard = registry.read().await;
            registry_guard.dns_metrics().record_operation_failure("create", "io_error", duration);
        }
        return Err(RecordError::IoError(io_error));
    }
    
    info!("Created new record with ID {}: {}", record_id, name);
    debug!("New record details: {:?}", new_record);
    
    // Record metrics for successful operation
    if let Some(registry) = &metrics_registry {
        let duration = start_time.elapsed().as_secs_f64();
        let registry_guard = registry.read().await;
        registry_guard.dns_metrics().record_operation_success("create", duration);
        registry_guard.dns_metrics().set_active_records_count(records_guard.len() as f64);
    }
    
    Ok(new_record)
}

/// Create a new DNS record from a CreateRecordRequest
/// Handles request validation, UUID generation, and returns HTTP 201 with created record details
pub async fn create_record_from_request(
    records: Arc<RwLock<DnsRecords>>, 
    create_request: CreateRecordRequest,
    metrics_registry: Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
) -> Result<DnsRecord, RecordError> {
    // Parse IP address if provided
    let ip = if let Some(ip_str) = create_request.ip {
        if ip_str.is_empty() {
            None
        } else {
            Some(ip_str.parse::<Ipv4Addr>()
                .map_err(|_| RecordError::ValidationError(
                    ValidationError::InvalidIpAddress(ip_str)
                ))?)
        }
    } else {
        None
    };
    
    // Use defaults for optional fields
    let ttl = create_request.ttl.unwrap_or(300); // Default TTL of 5 minutes
    let record_type = create_request.record_type.unwrap_or_else(|| "A".to_string()); // Default to A record
    let class = create_request.class.unwrap_or_else(|| "IN".to_string()); // Default to IN class
    
    // Call the main create_record function
    create_record(
        records,
        create_request.name,
        ip,
        ttl,
        record_type,
        class,
        create_request.value,
        metrics_registry,
    ).await
}

/// Retrieve a specific DNS record by its UUID
/// Uses efficient HashMap lookup for O(1) retrieval
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
            
            // Record metrics for successful operation
            if let Some(registry) = &metrics_registry {
                let duration = start_time.elapsed().as_secs_f64();
                let registry_guard = registry.read().await;
                registry_guard.dns_metrics().record_operation_success("read", duration);
            }
            
            Ok(record.clone())
        }
        None => {
            debug!("Record not found with ID: {}", id);
            
            // Record metrics for failed operation
            if let Some(registry) = &metrics_registry {
                let duration = start_time.elapsed().as_secs_f64();
                let registry_guard = registry.read().await;
                registry_guard.dns_metrics().record_operation_failure("read", "not_found", duration);
            }
            
            Err(RecordError::NotFound(format!("Record with ID '{}' not found", id)))
        }
    }
}

/// List DNS records with pagination support
/// Provides efficient pagination using HashMap indexing by UUID
pub async fn list_records(
    records: Arc<RwLock<DnsRecords>>, 
    page: usize, 
    per_page: usize,
    metrics_registry: Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
) -> Result<RecordListResponse, RecordError> {
    let start_time = std::time::Instant::now();
    let records_guard = records.read().await;
    
    // Validate pagination parameters
    if page == 0 {
        // Record metrics for failed operation
        if let Some(registry) = &metrics_registry {
            let duration = start_time.elapsed().as_secs_f64();
            let registry_guard = registry.read().await;
            registry_guard.dns_metrics().record_operation_failure("list", "validation_error", duration);
        }
        return Err(RecordError::ValidationError(ValidationError::InvalidTtl(
            "Page number must be greater than 0".to_string()
        )));
    }
    
    if per_page == 0 || per_page > 1000 {
        // Record metrics for failed operation
        if let Some(registry) = &metrics_registry {
            let duration = start_time.elapsed().as_secs_f64();
            let registry_guard = registry.read().await;
            registry_guard.dns_metrics().record_operation_failure("list", "validation_error", duration);
        }
        return Err(RecordError::ValidationError(ValidationError::InvalidTtl(
            "Per page must be between 1 and 1000".to_string()
        )));
    }
    
    let total = records_guard.len();
    let start_index = (page - 1) * per_page;
    
    // Collect records into a vector for pagination
    let mut all_records: Vec<DnsRecord> = records_guard.values().cloned().collect();
    
    // Sort by creation time for consistent ordering
    all_records.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    
    // Apply pagination
    let paginated_records: Vec<DnsRecord> = all_records
        .into_iter()
        .skip(start_index)
        .take(per_page)
        .collect();
    
    debug!("Listed {} records (page {}, per_page {})", paginated_records.len(), page, per_page);
    
    // Record metrics for successful operation
    if let Some(registry) = &metrics_registry {
        let duration = start_time.elapsed().as_secs_f64();
        let registry_guard = registry.read().await;
        registry_guard.dns_metrics().record_operation_success("list", duration);
    }
    
    Ok(RecordListResponse {
        records: paginated_records,
        total,
        page,
        per_page,
    })
}

/// Update an existing DNS record with proper error handling and partial update support
/// Accepts ID parameter and returns Result for proper error handling
pub async fn update_record(
    records: Arc<RwLock<DnsRecords>>, 
    id: &str, 
    update_request: UpdateRecordRequest,
    metrics_registry: Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
) -> Result<DnsRecord, RecordError> {
    let start_time = std::time::Instant::now();
    let mut records_guard = records.write().await;
    
    // Find the existing record
    let mut existing_record = match records_guard.get(id) {
        Some(record) => record.clone(),
        None => {
            // Record metrics for failed operation
            if let Some(registry) = &metrics_registry {
                let duration = start_time.elapsed().as_secs_f64();
                let registry_guard = registry.read().await;
                registry_guard.dns_metrics().record_operation_failure("update", "not_found", duration);
            }
            return Err(RecordError::NotFound(format!("Record with ID '{}' not found", id)));
        }
    };
    
    // Apply partial updates - only update fields that are provided
    if let Some(name) = update_request.name {
        existing_record.name = name;
    }
    
    if let Some(ip_str) = update_request.ip {
        if ip_str.is_empty() {
            existing_record.ip = None;
        } else {
            existing_record.ip = Some(ip_str.parse::<Ipv4Addr>()
                .map_err(|_| RecordError::ValidationError(
                    ValidationError::InvalidIpAddress(ip_str)
                ))?);
        }
    }
    
    if let Some(ttl) = update_request.ttl {
        existing_record.ttl = ttl;
    }
    
    if let Some(record_type) = update_request.record_type {
        existing_record.record_type = record_type;
    }
    
    if let Some(class) = update_request.class {
        existing_record.class = class;
    }
    
    if let Some(value) = update_request.value {
        existing_record.value = if value.is_empty() { None } else { Some(value) };
    }
    
    // Validate the updated record
    if let Err(validation_error) = existing_record.validate() {
        // Record metrics for failed operation
        if let Some(registry) = &metrics_registry {
            let duration = start_time.elapsed().as_secs_f64();
            let registry_guard = registry.read().await;
            registry_guard.dns_metrics().record_operation_failure("update", "validation_error", duration);
        }
        return Err(RecordError::ValidationError(validation_error));
    }
    
    // Check for duplicate names (excluding the current record)
    for (other_id, other_record) in records_guard.iter() {
        if other_id != id 
            && other_record.name == existing_record.name 
            && other_record.record_type == existing_record.record_type {
            // Record metrics for failed operation
            if let Some(registry) = &metrics_registry {
                let duration = start_time.elapsed().as_secs_f64();
                let registry_guard = registry.read().await;
                registry_guard.dns_metrics().record_operation_failure("update", "duplicate_record", duration);
            }
            return Err(RecordError::DuplicateRecord(format!(
                "Record with name '{}' and type '{}' already exists", 
                existing_record.name, 
                existing_record.record_type
            )));
        }
    }
    
    // Update the timestamp
    existing_record.touch();
    
    // Update the record in the HashMap
    records_guard.insert(id.to_string(), existing_record.clone());
    
    // Persist to file immediately
    if let Err(io_error) = save_records_to_file("dns_records.txt", &*records_guard) {
        // Record metrics for failed operation
        if let Some(registry) = &metrics_registry {
            let duration = start_time.elapsed().as_secs_f64();
            let registry_guard = registry.read().await;
            registry_guard.dns_metrics().record_operation_failure("update", "io_error", duration);
        }
        return Err(RecordError::IoError(io_error));
    }
    
    info!("Updated record with ID {}: {}", id, existing_record.name);
    debug!("Updated record details: {:?}", existing_record);
    
    // Record metrics for successful operation
    if let Some(registry) = &metrics_registry {
        let duration = start_time.elapsed().as_secs_f64();
        let registry_guard = registry.read().await;
        registry_guard.dns_metrics().record_operation_success("update", duration);
        registry_guard.dns_metrics().set_active_records_count(records_guard.len() as f64);
    }
    
    Ok(existing_record)
}

/// Delete a DNS record by its UUID
/// Provides proper error handling for non-existent records and immediate file persistence
pub async fn delete_record(
    records: Arc<RwLock<DnsRecords>>, 
    id: &str,
    metrics_registry: Option<Arc<RwLock<crate::metrics::MetricsRegistry>>>,
) -> Result<(), RecordError> {
    let start_time = std::time::Instant::now();
    let mut records_guard = records.write().await;
    
    // Check if the record exists before attempting to delete
    match records_guard.get(id) {
        Some(record) => {
            let record_name = record.name.clone();
            
            // Remove the record from the HashMap
            records_guard.remove(id);
            
            // Persist to file immediately
            if let Err(io_error) = save_records_to_file("dns_records.txt", &*records_guard) {
                // Record metrics for failed operation
                if let Some(registry) = &metrics_registry {
                    let duration = start_time.elapsed().as_secs_f64();
                    let registry_guard = registry.read().await;
                    registry_guard.dns_metrics().record_operation_failure("delete", "io_error", duration);
                }
                return Err(RecordError::IoError(io_error));
            }
            
            info!("Deleted record with ID {}: {}", id, record_name);
            debug!("Remaining records count: {}", records_guard.len());
            
            // Record metrics for successful operation
            if let Some(registry) = &metrics_registry {
                let duration = start_time.elapsed().as_secs_f64();
                let registry_guard = registry.read().await;
                registry_guard.dns_metrics().record_operation_success("delete", duration);
                registry_guard.dns_metrics().set_active_records_count(records_guard.len() as f64);
            }
            
            Ok(())
        }
        None => {
            debug!("Attempted to delete non-existent record with ID: {}", id);
            
            // Record metrics for failed operation
            if let Some(registry) = &metrics_registry {
                let duration = start_time.elapsed().as_secs_f64();
                let registry_guard = registry.read().await;
                registry_guard.dns_metrics().record_operation_failure("delete", "not_found", duration);
            }
            
            Err(RecordError::NotFound(format!("Record with ID '{}' not found", id)))
        }
    }
}

/// Legacy wrapper for update_record to maintain backward compatibility
/// This function maintains the old signature for existing code
pub async fn update_record_legacy(records: Arc<RwLock<DnsRecords>>, mut new_record: DnsRecord) {
    let mut records_guard = records.write().await;
    
    // Validate the record before updating
    if let Err(e) = new_record.validate() {
        error!("Record validation failed: {}", e);
        return;
    }
    
    // Update the timestamp
    new_record.touch();
    
    info!("Updating record: {:?}", new_record);
    records_guard.insert(new_record.id.clone(), new_record);
    debug!("Current records: {:?}", *records_guard);
    if let Err(e) = save_records_to_file("dns_records.txt", &*records_guard) {
        error!("Failed to save DNS records: {}", e);
    }
}