// Each `tests/*.rs` integration test is its own crate and only uses a subset
// of these helpers; the rest get flagged as dead by that crate's lint pass.
#![allow(dead_code)]

use async_trait::async_trait;
use rind::update::{DatastoreError, DatastoreProvider, DnsRecord, DnsRecords};
use std::sync::Mutex;

pub struct InMemoryDatastoreProvider {
    records: Mutex<DnsRecords>,
}

impl InMemoryDatastoreProvider {
    pub fn new() -> Self {
        Self {
            records: Mutex::new(DnsRecords::new()),
        }
    }

    pub fn with_records(records: DnsRecords) -> Self {
        Self {
            records: Mutex::new(records),
        }
    }
}

#[async_trait]
impl DatastoreProvider for InMemoryDatastoreProvider {
    async fn initialize(&self) -> Result<(), DatastoreError> {
        Ok(())
    }

    async fn load_all_records(&self) -> Result<DnsRecords, DatastoreError> {
        Ok(self.records.lock().unwrap().clone())
    }

    async fn put_record(&self, record: &DnsRecord) -> Result<(), DatastoreError> {
        self.records
            .lock()
            .unwrap()
            .insert(record.id.clone(), record.clone());
        Ok(())
    }

    async fn delete_record(&self, id: &str) -> Result<(), DatastoreError> {
        let mut guard = self.records.lock().unwrap();
        if guard.remove(id).is_none() {
            return Err(DatastoreError::NotFound(id.to_string()));
        }
        Ok(())
    }
}
