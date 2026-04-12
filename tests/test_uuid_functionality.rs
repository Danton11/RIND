use rind::update::{generate_record_id, DnsRecord, RecordData};
use std::net::{Ipv4Addr, Ipv6Addr};

fn a_record(name: &str) -> DnsRecord {
    DnsRecord::new(
        name.to_string(),
        300,
        "IN".to_string(),
        RecordData::A {
            ip: Ipv4Addr::new(93, 184, 216, 34),
        },
    )
}

#[test]
fn test_uuid_generation() {
    let id1 = generate_record_id();
    let id2 = generate_record_id();
    assert_ne!(id1, id2, "Generated UUIDs should be unique");
    assert_eq!(id1.len(), 36, "UUID should be 36 characters long");
    assert_eq!(id2.len(), 36, "UUID should be 36 characters long");
}

#[test]
fn test_dns_record_new_constructor() {
    let record = a_record("example.com");

    assert!(!record.id.is_empty(), "Record should have a UUID");
    assert_eq!(record.name, "example.com");
    assert_eq!(record.ttl, 300);
    assert_eq!(record.class, "IN");
    assert_eq!(
        record.data,
        RecordData::A {
            ip: Ipv4Addr::new(93, 184, 216, 34)
        }
    );
    assert!(record.created_at <= record.updated_at);
}

#[test]
fn test_dns_record_aaaa_variant() {
    let record = DnsRecord::new(
        "v6.example.com".to_string(),
        300,
        "IN".to_string(),
        RecordData::Aaaa {
            ip: "2001:db8::1".parse::<Ipv6Addr>().unwrap(),
        },
    );
    assert_eq!(record.data.type_name(), "AAAA");
    assert_eq!(record.data.type_code(), 28);
}

#[test]
fn test_record_validation_valid() {
    let record = a_record("example.com");
    assert!(
        record.validate().is_ok(),
        "Valid record should pass validation"
    );
}

#[test]
fn test_record_validation_invalid_empty_name() {
    let record = DnsRecord::new(
        "".to_string(),
        300,
        "IN".to_string(),
        RecordData::A {
            ip: Ipv4Addr::new(1, 2, 3, 4),
        },
    );
    assert!(
        record.validate().is_err(),
        "Record with empty name should fail validation"
    );
}

#[test]
fn test_record_validation_invalid_class() {
    let record = DnsRecord::new(
        "example.com".to_string(),
        300,
        "NOPE".to_string(),
        RecordData::A {
            ip: Ipv4Addr::new(1, 2, 3, 4),
        },
    );
    assert!(
        record.validate().is_err(),
        "Record with invalid class should fail validation"
    );
}

#[test]
fn test_record_touch_updates_timestamp() {
    let mut record = a_record("example.com");
    let original_updated_at = record.updated_at;

    std::thread::sleep(std::time::Duration::from_millis(10));
    record.touch();

    assert!(
        record.updated_at > original_updated_at,
        "touch() should update the timestamp"
    );
    assert_eq!(
        record.created_at, original_updated_at,
        "created_at should not change"
    );
}

#[test]
fn test_record_has_same_content() {
    let record1 = a_record("example.com");
    let record2 = a_record("example.com");

    assert!(
        record1.has_same_content(&record2),
        "Records with same content should be detected"
    );
    assert_ne!(record1.id, record2.id, "Records should have different IDs");
}

#[test]
fn test_record_serde_flatten_roundtrip() {
    // The flattened form means JSON looks like {"name":"...","type":"A","ip":"..."}
    let record = a_record("example.com");
    let json = serde_json::to_string(&record).unwrap();
    assert!(json.contains("\"type\":\"A\""));
    assert!(json.contains("\"ip\":\"93.184.216.34\""));
    let decoded: DnsRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.data, record.data);
}
