use reqwest;
use serde_json::json;
use std::error::Error;
use tokio;

const API_URL: &str = "http://127.0.0.1:8080/update";

#[derive(Debug)]
struct DnsRecord {
    name: String,
    ip: String,
    ttl: u32,
    record_type: String,
    class: String,
}

impl DnsRecord {
    fn new(name: &str, ip: &str, ttl: u32) -> Self {
        Self {
            name: name.to_string(),
            ip: ip.to_string(),
            ttl,
            record_type: "A".to_string(),
            class: "IN".to_string(),
        }
    }
}

async fn add_record(client: &reqwest::Client, record: &DnsRecord) -> Result<(), Box<dyn Error>> {
    let payload = json!({
        "name": record.name,
        "ip": record.ip,
        "ttl": record.ttl,
        "record_type": record.record_type,
        "class": record.class,
        "value": null
    });

    let response = client
        .post(API_URL)
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        println!("✅ Added: {} -> {} (TTL: {})", record.name, record.ip, record.ttl);
        Ok(())
    } else {
        println!("❌ Failed to add {}: HTTP {}", record.name, response.status());
        Err(format!("HTTP {}", response.status()).into())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🚀 Adding meaningful DNS records...");
    println!("{}", "=".repeat(50));

    let client = reqwest::Client::new();

    // Popular websites
    let records = vec![
        DnsRecord::new("facebook.com", "157.240.241.35", 300),
        DnsRecord::new("twitter.com", "104.244.42.129", 300),
        DnsRecord::new("youtube.com", "142.250.191.14", 300),
        DnsRecord::new("amazon.com", "205.251.242.103", 300),
        DnsRecord::new("netflix.com", "54.155.178.5", 300),
        
        // Development/Tech sites
        DnsRecord::new("stackoverflow.com", "151.101.1.69", 600),
        DnsRecord::new("reddit.com", "151.101.65.140", 300),
        DnsRecord::new("docker.com", "44.192.134.240", 300),
        DnsRecord::new("kubernetes.io", "147.75.40.148", 300),
        
        // Internal/Private network examples
        DnsRecord::new("mail.company.local", "192.168.1.10", 3600),
        DnsRecord::new("web.company.local", "192.168.1.20", 3600),
        DnsRecord::new("db.company.local", "192.168.1.30", 7200),
        DnsRecord::new("backup.company.local", "192.168.1.40", 86400),
        
        // Test domains with different TTLs
        DnsRecord::new("short-ttl.test", "1.1.1.1", 60),
        DnsRecord::new("long-ttl.test", "8.8.4.4", 86400),
        
        // CDN/Edge servers
        DnsRecord::new("cdn1.mysite.com", "203.0.113.10", 300),
        DnsRecord::new("cdn2.mysite.com", "203.0.113.20", 300),
        DnsRecord::new("edge.mysite.com", "203.0.113.30", 300),
    ];

    let mut successful = 0;
    let mut failed = 0;

    for record in &records {
        match add_record(&client, record).await {
            Ok(_) => successful += 1,
            Err(_) => failed += 1,
        }
    }

    println!("{}", "=".repeat(50));
    println!("✅ Successfully added: {} records", successful);
    println!("❌ Failed to add: {} records", failed);
    println!("📊 Total records processed: {}", records.len());

    Ok(())
}