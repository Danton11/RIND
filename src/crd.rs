use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, Ipv6Addr};

use crate::update;

#[derive(CustomResource, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[kube(
    group = "dns.rind.dev",
    version = "v1alpha1",
    kind = "DnsRecord",
    namespaced,
    status = "DnsRecordStatus",
    printcolumn = r#"{"name":"DNS Name","type":"string","jsonPath":".spec.name"}"#,
    printcolumn = r#"{"name":"Type","type":"string","jsonPath":".spec.recordData.type"}"#,
    printcolumn = r#"{"name":"TTL","type":"integer","jsonPath":".spec.ttl"}"#,
    printcolumn = r#"{"name":"Synced","type":"boolean","jsonPath":".status.synced"}"#
)]
pub struct DnsRecordSpec {
    pub name: String,
    #[serde(default = "default_ttl")]
    pub ttl: u32,
    #[serde(default = "default_class")]
    pub class: String,
    #[serde(rename = "recordData")]
    pub record_data: CrdRecordData,
}

fn default_ttl() -> u32 {
    300
}

fn default_class() -> String {
    "IN".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DnsRecordStatus {
    pub synced: bool,
    pub last_synced_at: Option<String>,
    pub error: Option<String>,
}

/// Record data as represented in the CRD spec. Uses an externally-tagged
/// `type` discriminator matching the DNS record type names.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum CrdRecordData {
    A {
        ip: String,
    },
    AAAA {
        ip: String,
    },
    CNAME {
        target: String,
    },
    PTR {
        target: String,
    },
    NS {
        target: String,
    },
    MX {
        preference: u16,
        exchange: String,
    },
    TXT {
        strings: Vec<String>,
    },
    SOA {
        mname: String,
        rname: String,
        serial: u32,
        refresh: u32,
        retry: u32,
        expire: u32,
        minimum: u32,
    },
    SRV {
        priority: u16,
        weight: u16,
        port: u16,
        target: String,
    },
}

impl DnsRecordSpec {
    /// Convert a CRD spec + K8s resource name (UUID) into an internal DnsRecord.
    pub fn to_dns_record(&self, id: &str) -> Result<update::DnsRecord, ConversionError> {
        let data = self.record_data.to_internal()?;
        let now = chrono::Utc::now();
        Ok(update::DnsRecord {
            id: id.to_string(),
            name: self.name.clone(),
            ttl: self.ttl,
            class: self.class.clone(),
            data,
            created_at: now,
            updated_at: now,
        })
    }
}

impl CrdRecordData {
    pub fn to_internal(&self) -> Result<update::RecordData, ConversionError> {
        match self {
            CrdRecordData::A { ip } => {
                let addr: Ipv4Addr = ip
                    .parse()
                    .map_err(|_| ConversionError::InvalidField(format!("invalid IPv4: {}", ip)))?;
                Ok(update::RecordData::A { ip: addr })
            }
            CrdRecordData::AAAA { ip } => {
                let addr: Ipv6Addr = ip
                    .parse()
                    .map_err(|_| ConversionError::InvalidField(format!("invalid IPv6: {}", ip)))?;
                Ok(update::RecordData::Aaaa { ip: addr })
            }
            CrdRecordData::CNAME { target } => Ok(update::RecordData::Cname {
                target: target.clone(),
            }),
            CrdRecordData::PTR { target } => Ok(update::RecordData::Ptr {
                target: target.clone(),
            }),
            CrdRecordData::NS { target } => Ok(update::RecordData::Ns {
                target: target.clone(),
            }),
            CrdRecordData::MX {
                preference,
                exchange,
            } => Ok(update::RecordData::Mx {
                preference: *preference,
                exchange: exchange.clone(),
            }),
            CrdRecordData::TXT { strings } => Ok(update::RecordData::Txt {
                strings: strings.clone(),
            }),
            CrdRecordData::SOA {
                mname,
                rname,
                serial,
                refresh,
                retry,
                expire,
                minimum,
            } => Ok(update::RecordData::Soa {
                mname: mname.clone(),
                rname: rname.clone(),
                serial: *serial,
                refresh: *refresh,
                retry: *retry,
                expire: *expire,
                minimum: *minimum,
            }),
            CrdRecordData::SRV {
                priority,
                weight,
                port,
                target,
            } => Ok(update::RecordData::Srv {
                priority: *priority,
                weight: *weight,
                port: *port,
                target: target.clone(),
            }),
        }
    }

    pub fn from_internal(data: &update::RecordData) -> Self {
        match data {
            update::RecordData::A { ip } => CrdRecordData::A { ip: ip.to_string() },
            update::RecordData::Aaaa { ip } => CrdRecordData::AAAA { ip: ip.to_string() },
            update::RecordData::Cname { target } => CrdRecordData::CNAME {
                target: target.clone(),
            },
            update::RecordData::Ptr { target } => CrdRecordData::PTR {
                target: target.clone(),
            },
            update::RecordData::Ns { target } => CrdRecordData::NS {
                target: target.clone(),
            },
            update::RecordData::Mx {
                preference,
                exchange,
            } => CrdRecordData::MX {
                preference: *preference,
                exchange: exchange.clone(),
            },
            update::RecordData::Txt { strings } => CrdRecordData::TXT {
                strings: strings.clone(),
            },
            update::RecordData::Soa {
                mname,
                rname,
                serial,
                refresh,
                retry,
                expire,
                minimum,
            } => CrdRecordData::SOA {
                mname: mname.clone(),
                rname: rname.clone(),
                serial: *serial,
                refresh: *refresh,
                retry: *retry,
                expire: *expire,
                minimum: *minimum,
            },
            update::RecordData::Srv {
                priority,
                weight,
                port,
                target,
            } => CrdRecordData::SRV {
                priority: *priority,
                weight: *weight,
                port: *port,
                target: target.clone(),
            },
        }
    }
}

impl DnsRecordSpec {
    pub fn from_dns_record(record: &update::DnsRecord) -> Self {
        Self {
            name: record.name.clone(),
            ttl: record.ttl,
            class: record.class.clone(),
            record_data: CrdRecordData::from_internal(&record.data),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("invalid field: {0}")]
    InvalidField(String),
}

use kube::api::{Api, DeleteParams, ObjectMeta, PatchParams, PostParams};
use kube::Client;
use std::sync::Arc;

use crate::storage::LmdbStore;

/// Proxies REST API writes to the K8s API. Cross-record conflict checks
/// (CNAME-over-A, RRSet duplicates) are run against the local LMDB cache
/// before patching K8s — same rules as the standalone path. There's a
/// TOCTOU window between any two pods because the local cache lags etcd by
/// the watcher's propagation delay; that's documented and accepted as an
/// eventual-consistency tradeoff. A validating admission webhook would close
/// the window cluster-wide; out of scope for now.
#[derive(Clone)]
pub struct KubeWriteClient {
    api: Api<DnsRecord>,
    store: Arc<LmdbStore>,
}

#[derive(Debug, thiserror::Error)]
pub enum KubeWriteError {
    #[error("kubernetes API error: {0}")]
    Kube(#[from] kube::Error),
    #[error("conversion error: {0}")]
    Conversion(#[from] ConversionError),
    #[error("validation error: {0}")]
    Validation(#[from] update::ValidationError),
    #[error("record error: {0}")]
    Record(#[from] update::RecordError),
}

impl KubeWriteError {
    /// HTTP status to return for this error. Validation / conflict errors
    /// surface as 4xx (matching the standalone path); kube transport errors
    /// stay 500.
    pub fn to_status_code(&self) -> u16 {
        match self {
            KubeWriteError::Validation(_) | KubeWriteError::Conversion(_) => 400,
            KubeWriteError::Record(e) => e.to_status_code(),
            KubeWriteError::Kube(_) => 500,
        }
    }
}

impl KubeWriteClient {
    pub fn new(client: Client, namespace: &str, store: Arc<LmdbStore>) -> Self {
        Self {
            api: Api::namespaced(client, namespace),
            store,
        }
    }

    /// Create a DnsRecord CRD object. Returns the internal DnsRecord
    /// representation immediately from the K8s API response.
    pub async fn create(
        &self,
        req: &update::CreateRecordRequest,
    ) -> Result<update::DnsRecord, KubeWriteError> {
        let ttl = req.ttl.unwrap_or(300);
        let class = req.class.clone().unwrap_or_else(|| "IN".to_string());

        let temp_record = update::DnsRecord::new(req.name.clone(), ttl, class, req.data.clone());
        update::validate_against_store(&self.store, &temp_record, None)?;

        let resource_name = &temp_record.id;
        let spec = DnsRecordSpec::from_dns_record(&temp_record);

        let cr = DnsRecord {
            metadata: ObjectMeta {
                name: Some(resource_name.clone()),
                labels: Some(std::collections::BTreeMap::from([
                    ("dns.rind.dev/record-name".to_string(), req.name.clone()),
                    (
                        "dns.rind.dev/record-type".to_string(),
                        req.data.type_name().to_string(),
                    ),
                ])),
                ..Default::default()
            },
            spec,
            status: None,
        };

        let result = self.api.create(&PostParams::default(), &cr).await?;
        let id = result.metadata.name.unwrap_or_default();
        Ok(result.spec.to_dns_record(&id)?)
    }

    /// Update (patch) an existing DnsRecord CRD object.
    pub async fn update(
        &self,
        id: &str,
        req: &update::UpdateRecordRequest,
    ) -> Result<update::DnsRecord, KubeWriteError> {
        let existing = self.api.get(id).await?;
        let mut spec = existing.spec;

        if let Some(ref name) = req.name {
            spec.name = name.clone();
        }
        if let Some(ttl) = req.ttl {
            spec.ttl = ttl;
        }
        if let Some(ref class) = req.class {
            spec.class = class.clone();
        }
        if let Some(ref data) = req.data {
            spec.record_data = CrdRecordData::from_internal(data);
        }

        let temp = spec.to_dns_record(id)?;
        update::validate_against_store(&self.store, &temp, Some(id))?;

        let patch = serde_json::json!({ "spec": spec });
        let result = self
            .api
            .patch(
                id,
                &PatchParams::apply("rind-api"),
                &kube::api::Patch::Merge(&patch),
            )
            .await?;
        let result_id = result.metadata.name.unwrap_or_default();
        Ok(result.spec.to_dns_record(&result_id)?)
    }

    /// Delete a DnsRecord CRD object.
    pub async fn delete(&self, id: &str) -> Result<(), KubeWriteError> {
        self.api.delete(id, &DeleteParams::default()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_a_record_roundtrip() {
        let internal = update::DnsRecord::new(
            "example.com".to_string(),
            300,
            "IN".to_string(),
            update::RecordData::A {
                ip: Ipv4Addr::new(10, 0, 1, 50),
            },
        );

        let spec = DnsRecordSpec::from_dns_record(&internal);
        let converted = spec.to_dns_record(&internal.id).unwrap();

        assert_eq!(converted.name, internal.name);
        assert_eq!(converted.ttl, internal.ttl);
        assert_eq!(converted.class, internal.class);
        assert_eq!(converted.data, internal.data);
        assert_eq!(converted.id, internal.id);
    }

    #[test]
    fn test_aaaa_record_roundtrip() {
        let internal = update::DnsRecord::new(
            "v6.example.com".to_string(),
            600,
            "IN".to_string(),
            update::RecordData::Aaaa {
                ip: Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1),
            },
        );

        let spec = DnsRecordSpec::from_dns_record(&internal);
        let converted = spec.to_dns_record(&internal.id).unwrap();

        assert_eq!(converted.data, internal.data);
    }

    #[test]
    fn test_cname_record_roundtrip() {
        let internal = update::DnsRecord::new(
            "www.example.com".to_string(),
            3600,
            "IN".to_string(),
            update::RecordData::Cname {
                target: "example.com".to_string(),
            },
        );

        let spec = DnsRecordSpec::from_dns_record(&internal);
        let converted = spec.to_dns_record(&internal.id).unwrap();

        assert_eq!(converted.data, internal.data);
    }

    #[test]
    fn test_mx_record_roundtrip() {
        let internal = update::DnsRecord::new(
            "example.com".to_string(),
            3600,
            "IN".to_string(),
            update::RecordData::Mx {
                preference: 10,
                exchange: "mail.example.com".to_string(),
            },
        );

        let spec = DnsRecordSpec::from_dns_record(&internal);
        let converted = spec.to_dns_record(&internal.id).unwrap();

        assert_eq!(converted.data, internal.data);
    }

    #[test]
    fn test_txt_record_roundtrip() {
        let internal = update::DnsRecord::new(
            "example.com".to_string(),
            300,
            "IN".to_string(),
            update::RecordData::Txt {
                strings: vec!["v=spf1 include:example.com ~all".to_string()],
            },
        );

        let spec = DnsRecordSpec::from_dns_record(&internal);
        let converted = spec.to_dns_record(&internal.id).unwrap();

        assert_eq!(converted.data, internal.data);
    }

    #[test]
    fn test_ns_record_roundtrip() {
        let internal = update::DnsRecord::new(
            "example.com".to_string(),
            86400,
            "IN".to_string(),
            update::RecordData::Ns {
                target: "ns1.example.com".to_string(),
            },
        );

        let spec = DnsRecordSpec::from_dns_record(&internal);
        let converted = spec.to_dns_record(&internal.id).unwrap();

        assert_eq!(converted.data, internal.data);
    }

    #[test]
    fn test_ptr_record_roundtrip() {
        let internal = update::DnsRecord::new(
            "50.1.0.10.in-addr.arpa".to_string(),
            300,
            "IN".to_string(),
            update::RecordData::Ptr {
                target: "host.example.com".to_string(),
            },
        );

        let spec = DnsRecordSpec::from_dns_record(&internal);
        let converted = spec.to_dns_record(&internal.id).unwrap();

        assert_eq!(converted.data, internal.data);
    }

    #[test]
    fn test_invalid_ipv4_conversion() {
        let crd_data = CrdRecordData::A {
            ip: "not.an.ip".to_string(),
        };
        let result = crd_data.to_internal();
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_ipv6_conversion() {
        let crd_data = CrdRecordData::AAAA {
            ip: "not-ipv6".to_string(),
        };
        let result = crd_data.to_internal();
        assert!(result.is_err());
    }

    #[test]
    fn test_default_ttl_and_class() {
        let spec = DnsRecordSpec {
            name: "test.com".to_string(),
            ttl: default_ttl(),
            class: default_class(),
            record_data: CrdRecordData::A {
                ip: "1.2.3.4".to_string(),
            },
        };

        assert_eq!(spec.ttl, 300);
        assert_eq!(spec.class, "IN");
    }

    #[test]
    fn test_crd_record_data_json_serialization() {
        let data = CrdRecordData::A {
            ip: "10.0.0.1".to_string(),
        };
        let json = serde_json::to_value(&data).unwrap();
        assert_eq!(json["type"], "A");
        assert_eq!(json["ip"], "10.0.0.1");

        let deserialized: CrdRecordData = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, data);
    }

    /// Single source of truth for "the record types we serve". If a new
    /// record type lands, every list below has to grow together — and so do
    /// `update::RecordData`, the wire encoder in `packet.rs`, and the CRD
    /// schemas. Keeping this list small and explicit makes those follow-on
    /// edits impossible to miss.
    const EXPECTED_RECORD_TYPES: &[&str] =
        &["A", "AAAA", "CNAME", "PTR", "NS", "MX", "TXT", "SOA", "SRV"];

    /// Every variant of `CrdRecordData` must round-trip through the type
    /// discriminator we ship in the CRD schema. This catches the
    /// "added a Rust variant, forgot the CRD" half of drift.
    #[test]
    fn every_crd_record_data_variant_serializes_to_an_expected_type() {
        // Construct one of each variant with arbitrary-but-valid payloads,
        // then collect the type tags serde emits.
        let samples = [
            CrdRecordData::A {
                ip: "1.2.3.4".to_string(),
            },
            CrdRecordData::AAAA {
                ip: "::1".to_string(),
            },
            CrdRecordData::CNAME {
                target: "x".to_string(),
            },
            CrdRecordData::PTR {
                target: "x".to_string(),
            },
            CrdRecordData::NS {
                target: "x".to_string(),
            },
            CrdRecordData::MX {
                preference: 10,
                exchange: "x".to_string(),
            },
            CrdRecordData::TXT {
                strings: vec!["x".to_string()],
            },
            CrdRecordData::SOA {
                mname: "x".to_string(),
                rname: "x".to_string(),
                serial: 1,
                refresh: 1,
                retry: 1,
                expire: 1,
                minimum: 1,
            },
            CrdRecordData::SRV {
                priority: 0,
                weight: 0,
                port: 0,
                target: "x".to_string(),
            },
        ];
        let mut emitted: Vec<String> = samples
            .iter()
            .map(|s| {
                serde_json::to_value(s).unwrap()["type"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        emitted.sort();
        let mut expected: Vec<String> = EXPECTED_RECORD_TYPES
            .iter()
            .map(|s| s.to_string())
            .collect();
        expected.sort();
        assert_eq!(
            emitted, expected,
            "CrdRecordData variants drifted from EXPECTED_RECORD_TYPES — \
             update both lists together"
        );
    }

    /// The kustomize and Helm CRD schemas must enumerate exactly the types
    /// in `EXPECTED_RECORD_TYPES`. Catches the "added a Rust variant, forgot
    /// the YAML" half of drift, and the "kustomize and chart fell out of
    /// sync with each other" case.
    #[test]
    fn crd_yaml_enum_matches_expected_record_types() {
        for path in [
            "k8s/base/crd-dnsrecord.yaml",
            "charts/rind/templates/crd.yaml",
        ] {
            let yaml =
                std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {}", path, e));
            // The schema enum is the only line that lists the type tokens
            // back-to-back like this. If we ever wrap it across lines, this
            // test breaks loudly — that's fine, it just needs updating.
            let enum_line = yaml
                .lines()
                .find(|l| l.contains("enum:") && l.contains("\"A\"") && l.contains("\"AAAA\""))
                .unwrap_or_else(|| panic!("no recordData type enum line found in {}", path));
            for ty in EXPECTED_RECORD_TYPES {
                assert!(
                    enum_line.contains(&format!("\"{}\"", ty)),
                    "{} is missing record type {} in its enum line: {}",
                    path,
                    ty,
                    enum_line
                );
            }
        }
    }
}
