use std::io::Write;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use tempfile::NamedTempFile;

use rind::update::{load_records_from_file, save_records_to_file, DnsRecord, DnsRecords};

#[test]
fn test_load_records_from_file_valid() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "test.com:1.2.3.4:60:A:IN").unwrap();
    let path = file.path().to_str().unwrap();
    let records = load_records_from_file(path).unwrap();
    assert!(records.contains_key("test.com"));
    let rec = records.get("test.com").unwrap();
    assert_eq!(rec.ip, Some(Ipv4Addr::new(1,2,3,4)));
    assert_eq!(rec.ttl, 60);
    assert_eq!(rec.record_type, "A");
    assert_eq!(rec.class, "IN");
}

#[test]
fn test_save_and_load_records_to_file() {
    let mut records = DnsRecords::new();
    records.insert("foo.com".to_string(), DnsRecord {
        name: "foo.com".to_string(),
        ip: Some(Ipv4Addr::new(5,6,7,8)),
        ttl: 120,
        record_type: "A".to_string(),
        class: "IN".to_string(),
        value: None,
    });
    let file = NamedTempFile::new().unwrap();
    let path = file.path().to_str().unwrap();
    save_records_to_file(path, &records).unwrap();
    let loaded = load_records_from_file(path).unwrap();
    assert!(loaded.contains_key("foo.com"));
    let rec = loaded.get("foo.com").unwrap();
    assert_eq!(rec.ip, Some(Ipv4Addr::new(5,6,7,8)));
    assert_eq!(rec.ttl, 120);
}

#[test]
fn test_load_records_from_file_invalid() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "invalid_line_without_colons").unwrap();
    let path = file.path().to_str().unwrap();
    let result = load_records_from_file(path);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_update_record() {
    let rt = Runtime::new().unwrap();
    let file = NamedTempFile::new().unwrap();
    let path = file.path().to_str().unwrap();
    let records = Arc::new(RwLock::new(DnsRecords::new()));
    let new_record = DnsRecord {
        name: "bar.com".to_string(),
        ip: Some(Ipv4Addr::new(9,9,9,9)),
        ttl: 99,
        record_type: "A".to_string(),
        class: "IN".to_string(),
        value: None,
    };
    {
        let mut recs = rt.block_on(records.write());
        recs.insert(new_record.name.clone(), new_record.clone());
    }
    save_records_to_file(path, &rt.block_on(records.read())).unwrap();
    let loaded = load_records_from_file(path).unwrap();
    assert!(loaded.contains_key("bar.com"));
    let rec = loaded.get("bar.com").unwrap();
    assert_eq!(rec.ip, Some(Ipv4Addr::new(9,9,9,9)));
    assert_eq!(rec.ttl, 99);
}