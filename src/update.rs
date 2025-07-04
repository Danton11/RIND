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
            continue;
        }
        
        let parts: Vec<&str> = line.split(':').collect();
        
        // Standard A record: name:ip:ttl:type:class
        if parts.len() >= 5 {
            let ip = parts[1].parse::<Ipv4Addr>().ok();
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
        }
        // CNAME record
        else if parts.len() == 4 && parts[0].starts_with('C') {
            let record = DnsRecord {
                name: parts[0].to_string(),
                ip: None,
                ttl: parts[2].parse::<u32>()?,
                record_type: "CNAME".to_string(),
                class: parts[3].to_string(),
                value: Some(parts[1].to_string()),
            };
            records.insert(record.name.clone(), record);
        }
        // TXT record
        else if parts.len() == 5 && parts[0].starts_with('\'') {
            let record = DnsRecord {
                name: parts[0].to_string(),
                ip: None,
                ttl: parts[2].parse::<u32>()?,
                record_type: "TXT".to_string(),
                class: parts[4].to_string(),
                value: Some(parts[1].to_string()),
            };
            records.insert(record.name.clone(), record);
        } else {
            error!("Invalid record format: {}", line);
        }
    }
    Ok(records)
}

pub fn save_records_to_file(file_path: &str, records: &DnsRecords) -> Result<(), IoError> {
    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);

    for record in records.values() {
        let ip_or_value = record.ip
            .map(|ip| ip.to_string())
            .or_else(|| record.value.clone())
            .unwrap_or_default();
        
        let extra_value = record.value.as_ref()
            .map(|v| format!(":{}", v))
            .unwrap_or_default();

        writeln!(writer, "{}:{}:{}:{}:{}{}", 
            record.name, ip_or_value, record.ttl, 
            record.record_type, record.class, extra_value)?;
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



