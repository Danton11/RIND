use rind::update::{
    DatastoreProvider, DnsRecord, DnsRecords, JsonlFileDatastoreProvider, RecordData,
};
use std::fs;
use std::net::Ipv4Addr;
use tempfile::tempdir;

#[tokio::test]
async fn test_initialize_creates_empty_file() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("records.jsonl");
    let path_str = file_path.to_str().unwrap().to_string();

    let provider = JsonlFileDatastoreProvider::new(path_str.clone());
    provider.initialize().await.unwrap();

    assert!(file_path.exists(), "datastore file was not created");
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.is_empty(), "fresh datastore should start empty");
}

#[tokio::test]
async fn test_health_check_reflects_file_presence() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("records.jsonl");
    let provider = JsonlFileDatastoreProvider::new(file_path.to_str().unwrap().to_string());

    assert!(!provider.health_check().await.unwrap());
    provider.initialize().await.unwrap();
    assert!(provider.health_check().await.unwrap());
}

#[tokio::test]
async fn test_initialize_is_idempotent() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("records.jsonl");
    let provider = JsonlFileDatastoreProvider::new(file_path.to_str().unwrap().to_string());

    // Seed a record, then re-initialize — existing content must survive.
    let mut records = DnsRecords::new();
    let record = DnsRecord::new(
        "example.com".to_string(),
        300,
        "IN".to_string(),
        RecordData::A {
            ip: Ipv4Addr::new(1, 2, 3, 4),
        },
    );
    records.insert(record.id.clone(), record);
    provider.save_all_records(&records).await.unwrap();
    let before = fs::read_to_string(&file_path).unwrap();

    provider.initialize().await.unwrap();
    let after = fs::read_to_string(&file_path).unwrap();
    assert_eq!(before, after, "initialize must not clobber existing data");
}

#[tokio::test]
async fn test_save_and_load_roundtrip_via_provider() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("records.jsonl");
    let provider = JsonlFileDatastoreProvider::new(file_path.to_str().unwrap().to_string());

    let mut records = DnsRecords::new();
    let record = DnsRecord::new(
        "roundtrip.example.com".to_string(),
        600,
        "IN".to_string(),
        RecordData::A {
            ip: Ipv4Addr::new(10, 0, 0, 1),
        },
    );
    let id = record.id.clone();
    records.insert(id.clone(), record);

    provider.save_all_records(&records).await.unwrap();
    let loaded = provider.load_all_records().await.unwrap();
    let loaded_record = loaded.get(&id).expect("record should round-trip");
    assert_eq!(loaded_record.name, "roundtrip.example.com");
    assert_eq!(loaded_record.ttl, 600);
}

#[tokio::test]
async fn test_initialize_with_invalid_path() {
    let provider = JsonlFileDatastoreProvider::new(
        "/invalid/path/that/does/not/exist/records.jsonl".to_string(),
    );
    assert!(provider.initialize().await.is_err());
}
