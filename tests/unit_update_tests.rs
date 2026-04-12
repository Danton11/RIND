use std::net::Ipv4Addr;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;

use rind::update::{
    load_records_from_file, save_records_to_file, DnsRecord, DnsRecords, RecordData,
};

fn sample_a(name: &str, ip: [u8; 4], ttl: u32) -> DnsRecord {
    DnsRecord::new(
        name.to_string(),
        ttl,
        "IN".to_string(),
        RecordData::A {
            ip: Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]),
        },
    )
}

#[test]
fn test_save_and_load_records_roundtrip() {
    let mut records = DnsRecords::new();
    let record = sample_a("foo.com", [5, 6, 7, 8], 120);
    records.insert(record.id.clone(), record);

    let file = NamedTempFile::new().unwrap();
    let path = file.path().to_str().unwrap();
    save_records_to_file(path, &records).unwrap();

    let loaded = load_records_from_file(path).unwrap();
    let rec = loaded.values().find(|r| r.name == "foo.com").unwrap();
    assert_eq!(rec.ttl, 120);
    assert_eq!(
        rec.data,
        RecordData::A {
            ip: Ipv4Addr::new(5, 6, 7, 8)
        }
    );
}

#[test]
fn test_load_records_from_file_ignores_garbage_lines() {
    use std::io::Write;
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "# comment line").unwrap();
    writeln!(file, "this is not json").unwrap();
    writeln!(file).unwrap();
    let path = file.path().to_str().unwrap();
    let records = load_records_from_file(path).unwrap();
    assert!(records.is_empty());
}

#[test]
fn test_save_load_via_in_memory_cache() {
    let rt = Runtime::new().unwrap();
    let file = NamedTempFile::new().unwrap();
    let path = file.path().to_str().unwrap();
    let records = Arc::new(RwLock::new(DnsRecords::new()));
    let record = sample_a("bar.com", [9, 9, 9, 9], 99);
    {
        let mut recs = rt.block_on(records.write());
        recs.insert(record.id.clone(), record.clone());
    }
    save_records_to_file(path, &rt.block_on(records.read())).unwrap();
    let loaded = load_records_from_file(path).unwrap();
    let rec = loaded.values().find(|r| r.name == "bar.com").unwrap();
    assert_eq!(rec.ttl, 99);
    assert_eq!(
        rec.data,
        RecordData::A {
            ip: Ipv4Addr::new(9, 9, 9, 9)
        }
    );
}
