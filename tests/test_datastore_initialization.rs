use std::fs;
use tempfile::tempdir;
use rind::update::{initialize_empty_datastore, validate_datastore_format, ensure_datastore_initialized};

#[test]
fn test_initialize_empty_datastore() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test_records.txt");
    let file_path_str = file_path.to_str().unwrap();

    // Test successful initialization
    let result = initialize_empty_datastore(file_path_str);
    assert!(result.is_ok(), "Failed to initialize empty datastore: {:?}", result);

    // Verify file was created
    assert!(file_path.exists(), "Datastore file was not created");

    // Read and verify content
    let content = fs::read_to_string(&file_path).unwrap();
    
    // Check for header comments
    assert!(content.contains("DNS Records File - Enhanced UUID Format"));
    assert!(content.contains("Format: id:name:ip:ttl:type:class:value"));
    assert!(content.contains("id: UUID v4 identifier"));
    assert!(content.contains("Examples:"));
    assert!(content.contains("550e8400-e29b-41d4-a716-446655440000:example.com"));
    
    // Verify it ends with empty line for records
    assert!(content.ends_with("\n\n"));
}

#[test]
fn test_validate_datastore_format() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test_records.txt");
    let file_path_str = file_path.to_str().unwrap();

    // Test non-existent file
    let result = validate_datastore_format(file_path_str);
    assert!(result.is_ok());
    assert!(!result.unwrap(), "Non-existent file should return false");

    // Test properly formatted file
    initialize_empty_datastore(file_path_str).unwrap();
    let result = validate_datastore_format(file_path_str);
    assert!(result.is_ok());
    assert!(result.unwrap(), "Properly formatted file should return true");

    // Test file with UUID format record
    let uuid_content = "550e8400-e29b-41d4-a716-446655440000:example.com:93.184.216.34:300:A:IN\n";
    fs::write(&file_path, uuid_content).unwrap();
    let result = validate_datastore_format(file_path_str);
    assert!(result.is_ok());
    assert!(result.unwrap(), "File with UUID records should return true");

    // Test legacy format file
    let legacy_content = "example.com:93.184.216.34:300:A:IN\n";
    fs::write(&file_path, legacy_content).unwrap();
    let result = validate_datastore_format(file_path_str);
    assert!(result.is_ok());
    assert!(!result.unwrap(), "Legacy format file should return false");
}

#[test]
fn test_ensure_datastore_initialized() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test_records.txt");
    let file_path_str = file_path.to_str().unwrap();

    // Test initialization of non-existent file
    let result = ensure_datastore_initialized(file_path_str);
    assert!(result.is_ok(), "Failed to ensure datastore initialized: {:?}", result);
    assert!(file_path.exists(), "Datastore file should be created");

    // Verify content
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("DNS Records File - Enhanced UUID Format"));

    // Test with already initialized file (should not overwrite)
    let original_content = fs::read_to_string(&file_path).unwrap();
    let result = ensure_datastore_initialized(file_path_str);
    assert!(result.is_ok(), "Failed on already initialized file: {:?}", result);
    
    let new_content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(original_content, new_content, "File should not be overwritten");
}

#[test]
fn test_initialize_with_invalid_path() {
    // Test with invalid path (should fail gracefully)
    let result = initialize_empty_datastore("/invalid/path/that/does/not/exist/records.txt");
    assert!(result.is_err(), "Should fail with invalid path");
}