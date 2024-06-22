use std::collections::HashMap;
use std::sync::Arc;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write, BufRead, Error as IoError};
use tokio::sync::RwLock;
use std::net::Ipv4Addr;
use serde::{Deserialize, Serialize};
use log::{info, debug, error};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DnsRecord {
    pub name: String,
    pub ip: Option<Ipv4Addr>, // Make IP optional to handle non-IP records
    pub ttl: u32,
    pub record_type: String,
    pub class: String,
    pub value: Option<String>, // Additional field to handle non-IP values like CNAME or TXT
}

pub type DnsRecords = HashMap<String, DnsRecord>;

pub fn load_records_from_file(file_path: &str) -> Result<DnsRecords, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut records = DnsRecords::new();

    for line in reader.lines() {
        let line = line?.trim().to_string();
        if line.is_empty() || line.starts_with('#') {
            continue; // Skip empty lines and comments
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 5 {
            let ip = match parts[1].parse::<Ipv4Addr>() {
                Ok(ip) => Some(ip),
                Err(_) => None, // If IP parsing fails, set it to None
            };
            let ttl = parts[2].parse::<u32>().map_err(|e| {
                error!("Failed to parse TTL for line {}: {}", line, e);
                e
            })?;
            let record = DnsRecord {
                name: parts[0].to_string(),
                ip,
                ttl,
                record_type: parts[3].to_string(),
                class: parts[4].to_string(),
                value: None,
            };
            records.insert(record.name.clone(), record);
        } else if parts.len() == 4 && parts[0].starts_with('C') { // Special case for CNAME
            let record = DnsRecord {
                name: parts[0].to_string(),
                ip: None,
                ttl: parts[2].parse::<u32>().map_err(|e| {
                    error!("Failed to parse TTL for line {}: {}", line, e);
                    e
                })?,
                record_type: "CNAME".to_string(),
                class: parts[3].to_string(),
                value: Some(parts[1].to_string()), // CNAME value
            };
            records.insert(record.name.clone(), record);
        } else if parts.len() == 5 && parts[0].starts_with('\'') { // Special case for TXT
            let record = DnsRecord {
                name: parts[0].to_string(),
                ip: None,
                ttl: parts[2].parse::<u32>().map_err(|e| {
                    error!("Failed to parse TTL for line {}: {}", line, e);
                    e
                })?,
                record_type: "TXT".to_string(),
                class: parts[4].to_string(),
                value: Some(parts[1].to_string()), // TXT value
            };
            records.insert(record.name.clone(), record);
        } else {
            error!("Invalid record format for line: {}", line);
        }
    }
    Ok(records)
}

pub fn save_records_to_file(file_path: &str, records: &DnsRecords) -> Result<(), IoError> {
    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);

    for record in records.values() {
        writeln!(
            writer,
            "{}:{}:{}:{}:{}{}",
            record.name,
            record.ip.map_or_else(|| record.value.clone().unwrap_or_default(), |ip| ip.to_string()), // Handle None IP case
            record.ttl,
            record.record_type,
            record.class,
            record.value.as_ref().map_or("".to_string(), |v| format!(":{}", v))
        )?;
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

pub async fn update_record(records: Arc<RwLock<DnsRecords>>, new_record: DnsRecord) {
    let mut records = records.write().await;
    info!("Updating record: {:?}", new_record);
    records.insert(new_record.name.clone(), new_record);
    debug!("Current records: {:?}", *records);
    save_records_to_file("dns_records.txt", &*records).expect("Failed to save DNS records");
}

