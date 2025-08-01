use rind::update::{DnsRecord, generate_record_id};
use std::net::Ipv4Addr;

#[test]
fn test_uuid_generation() {
    // Test that UUIDs are generated and unique
    let id1 = generate_record_id();
    let id2 = generate_record_id();
    
    assert_ne!(id1, id2, "Generated UUIDs should be unique");
    assert_eq!(id1.len(), 36, "UUID should be 36 characters long");
    assert_eq!(id2.len(), 36, "UUID should be 36 characters long");
}

#[test]
fn test_dns_record_new_constructor() {
    let record = DnsRecord::new(
        "example.com".to_string(),
        Some(Ipv4Addr::new(93, 184, 216, 34)),
        300,
        "A".to_string(),
        "IN".to_string(),
        None,
    );

    assert!(!record.id.is_empty(), "Record should have a UUID");
    assert_eq!(record.name, "example.com");
    assert_eq!(record.ip, Some(Ipv4Addr::new(93, 184, 216, 34)));
    assert_eq!(record.ttl, 300);
    assert_eq!(record.record_type, "A");
    assert_eq!(record.class, "IN");
    assert!(record.value.is_none());
    
    // Timestamps should be set
    assert!(record.created_at <= record.updated_at);
}

#[test]
fn test_record_validation_valid() {
    let record = DnsRecord::new(
        "example.com".to_string(),
        Some(Ipv4Addr::new(93, 184, 216, 34)),
        300,
        "A".to_string(),
        "IN".to_string(),
        None,
    );

    assert!(record.validate().is_ok(), "Valid record should pass validation");
}

#[test]
fn test_record_validation_invalid_empty_name() {
    let record = DnsRecord::new(
        "".to_string(), // Empty name should fail
        Some(Ipv4Addr::new(1, 2, 3, 4)),
        300,
        "A".to_string(),
        "IN".to_string(),
        None,
    );

    assert!(record.validate().is_err(), "Record with empty name should fail validation");
}

#[test]
fn test_record_validation_invalid_record_type() {
    let record = DnsRecord::new(
        "example.com".to_string(),
        Some(Ipv4Addr::new(1, 2, 3, 4)),
        300,
        "INVALID".to_string(), // Invalid record type
        "IN".to_string(),
        None,
    );

    assert!(record.validate().is_err(), "Record with invalid type should fail validation");
}

#[test]
fn test_record_validation_a_record_without_ip() {
    let record = DnsRecord::new(
        "example.com".to_string(),
        None, // A record should have IP
        300,
        "A".to_string(),
        "IN".to_string(),
        None,
    );

    assert!(record.validate().is_err(), "A record without IP should fail validation");
}

#[test]
fn test_record_touch_updates_timestamp() {
    let mut record = DnsRecord::new(
        "example.com".to_string(),
        Some(Ipv4Addr::new(1, 2, 3, 4)),
        300,
        "A".to_string(),
        "IN".to_string(),
        None,
    );

    let original_updated_at = record.updated_at;
    
    // Sleep a bit to ensure timestamp difference
    std::thread::sleep(std::time::Duration::from_millis(10));
    record.touch();
    
    assert!(record.updated_at > original_updated_at, "touch() should update the timestamp");
    assert_eq!(record.created_at, original_updated_at, "created_at should not change");
}

#[test]
fn test_record_has_same_content() {
    let record1 = DnsRecord::new(
        "example.com".to_string(),
        Some(Ipv4Addr::new(1, 2, 3, 4)),
        300,
        "A".to_string(),
        "IN".to_string(),
        None,
    );

    let record2 = DnsRecord::new(
        "example.com".to_string(),
        Some(Ipv4Addr::new(1, 2, 3, 4)),
        300,
        "A".to_string(),
        "IN".to_string(),
        None,
    );

    // Should have same content despite different IDs and timestamps
    assert!(record1.has_same_content(&record2), "Records with same content should be detected");
    assert_ne!(record1.id, record2.id, "Records should have different IDs");
}