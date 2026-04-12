//! In-process test harness. Spawns a full RIND instance (DNS + REST) on
//! ephemeral ports backed by a per-test LMDB tempdir, so integration tests
//! run in milliseconds without needing docker-compose or a shared port.
//!
//! # Lifetime
//!
//! `TestHarness::spawn()` returns a handle that owns the DNS + API join
//! handles and the `TempDir`. On `Drop` the handles are aborted and the
//! tempdir is removed — one instance per test, fully isolated.

use std::net::SocketAddr;
use std::time::Duration;

use rind::instance::{build_instance, Instance, InstanceConfig};
use tempfile::TempDir;
use tokio::net::UdpSocket;

pub struct TestHarness {
    pub dns_addr: SocketAddr,
    pub api_base: String,
    pub http: reqwest::Client,
    instance: Option<Instance>,
    // Keep tempdir alive for the lifetime of the instance. Dropped after
    // `instance` via field order — LMDB env closes before the dir vanishes.
    _lmdb_dir: TempDir,
}

impl TestHarness {
    pub async fn spawn() -> Self {
        let lmdb_dir = tempfile::tempdir().expect("tempdir");
        let cfg = InstanceConfig {
            dns_bind: "127.0.0.1:0".parse().unwrap(),
            api_bind: "127.0.0.1:0".parse().unwrap(),
            lmdb_path: lmdb_dir.path().to_path_buf(),
            server_id: "test-harness".to_string(),
            metrics_bind: None,
        };
        let instance = build_instance(cfg).await.expect("build_instance");
        let dns_addr = instance.dns_addr;
        let api_base = format!("http://{}", instance.api_addr);

        Self {
            dns_addr,
            api_base,
            http: reqwest::Client::new(),
            instance: Some(instance),
            _lmdb_dir: lmdb_dir,
        }
    }

    /// POST /records with an arbitrary JSON body. Returns the raw response so
    /// tests can assert status + body shape. Use this for anything beyond the
    /// simple A-record shortcut.
    pub async fn post_record(&self, body: serde_json::Value) -> reqwest::Response {
        self.http
            .post(format!("{}/records", self.api_base))
            .json(&body)
            .send()
            .await
            .expect("POST /records")
    }

    /// PUT /records/{id} with an arbitrary JSON body.
    pub async fn put_record(&self, id: &str, body: serde_json::Value) -> reqwest::Response {
        self.http
            .put(format!("{}/records/{}", self.api_base, id))
            .json(&body)
            .send()
            .await
            .expect("PUT /records/{id}")
    }

    /// DELETE /records/{id}.
    pub async fn delete_record(&self, id: &str) -> reqwest::Response {
        self.http
            .delete(format!("{}/records/{}", self.api_base, id))
            .send()
            .await
            .expect("DELETE /records/{id}")
    }

    /// POST an A record and return the generated record id. Thin wrapper on
    /// `post_record` for the common case — tests that want to assert on
    /// failure paths should call `post_record` directly.
    pub async fn create_a(&self, name: &str, ip: &str) -> String {
        let resp = self
            .post_record(serde_json::json!({
                "name": name,
                "ttl": 300,
                "class": "IN",
                "type": "A",
                "ip": ip,
            }))
            .await;
        assert!(
            resp.status().is_success(),
            "POST /records failed: {}",
            resp.status()
        );
        let json: serde_json::Value = resp.json().await.expect("json");
        json["data"]["id"].as_str().expect("record id").to_string()
    }

    /// Fire an A-query packet at the DNS server and return raw response bytes.
    pub async fn query_a(&self, name: &str) -> Vec<u8> {
        self.query(name, 1).await
    }

    /// Fire a query of arbitrary qtype at the DNS server. Returns raw response
    /// bytes — each test decides what to assert on. A new client socket per
    /// query is fine; these are cheap.
    pub async fn query(&self, name: &str, qtype: u16) -> Vec<u8> {
        let socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind client");
        socket.connect(self.dns_addr).await.expect("connect");
        let packet = build_query(name, qtype);
        socket.send(&packet).await.expect("send");

        let mut buf = vec![0u8; 512];
        let len = tokio::time::timeout(Duration::from_secs(2), socket.recv(&mut buf))
            .await
            .expect("recv timeout")
            .expect("recv");
        buf.truncate(len);
        buf
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        if let Some(instance) = self.instance.take() {
            instance.dns_task.abort();
            instance.api_task.abort();
        }
    }
}

/// Minimal DNS query packet for `<name> IN <qtype>`. ID is fixed at 0x1234
/// because tests don't multiplex responses on one socket.
pub fn build_query(name: &str, qtype: u16) -> Vec<u8> {
    let mut packet = vec![
        0x12, 0x34, // ID
        0x01, 0x00, // Flags: standard query, RD
        0x00, 0x01, // QDCOUNT
        0x00, 0x00, // ANCOUNT
        0x00, 0x00, // NSCOUNT
        0x00, 0x00, // ARCOUNT
    ];
    for label in name.split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0); // root
    packet.extend(&qtype.to_be_bytes());
    packet.extend(&[0x00, 0x01]); // QCLASS IN
    packet
}

/// Back-compat shim — the concurrent-queries test captures `build_a_query`
/// into spawned tasks, so keep a thin wrapper.
pub fn build_a_query(name: &str) -> Vec<u8> {
    build_query(name, 1)
}

/// Extract the RCODE (low 4 bits of flags) from a DNS response.
pub fn rcode(response: &[u8]) -> u8 {
    response[3] & 0x0F
}

/// Extract ANCOUNT from a DNS response header.
pub fn ancount(response: &[u8]) -> u16 {
    u16::from_be_bytes([response[6], response[7]])
}
