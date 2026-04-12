use serde_json::json;
use std::error::Error;

fn get_api_url() -> String {
    let api_addr =
        std::env::var("API_SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    format!("http://{}/records", api_addr)
}

#[derive(Debug)]
struct SeedRecord {
    name: String,
    ip: String,
    ttl: u32,
}

impl SeedRecord {
    fn new(name: &str, ip: &str, ttl: u32) -> Self {
        Self {
            name: name.to_string(),
            ip: ip.to_string(),
            ttl,
        }
    }
}

async fn add_record(client: &reqwest::Client, record: &SeedRecord) -> Result<(), Box<dyn Error>> {
    // POST /records — new shape: flattened RecordData with `type` tag.
    let payload = json!({
        "name": record.name,
        "ttl": record.ttl,
        "class": "IN",
        "type": "A",
        "ip": record.ip,
    });

    let response = client.post(get_api_url()).json(&payload).send().await?;

    if response.status().is_success() {
        println!(
            "added: {} -> {} (TTL: {})",
            record.name, record.ip, record.ttl
        );
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        println!("failed to add {}: HTTP {} — {}", record.name, status, body);
        Err(format!("HTTP {}", status).into())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("seeding DNS records...");
    println!("{}", "=".repeat(50));

    let client = reqwest::Client::new();

    let records = vec![
        SeedRecord::new("facebook.com", "157.240.241.35", 300),
        SeedRecord::new("twitter.com", "104.244.42.129", 300),
        SeedRecord::new("youtube.com", "142.250.191.14", 300),
        SeedRecord::new("amazon.com", "205.251.242.103", 300),
        SeedRecord::new("netflix.com", "54.155.178.5", 300),
        SeedRecord::new("stackoverflow.com", "151.101.1.69", 600),
        SeedRecord::new("reddit.com", "151.101.65.140", 300),
        SeedRecord::new("docker.com", "44.192.134.240", 300),
        SeedRecord::new("kubernetes.io", "147.75.40.148", 300),
        SeedRecord::new("mail.company.local", "192.168.1.10", 3600),
        SeedRecord::new("web.company.local", "192.168.1.20", 3600),
        SeedRecord::new("db.company.local", "192.168.1.30", 7200),
        SeedRecord::new("backup.company.local", "192.168.1.40", 86400),
        SeedRecord::new("short-ttl.test", "1.1.1.1", 60),
        SeedRecord::new("long-ttl.test", "8.8.4.4", 86400),
        SeedRecord::new("cdn1.mysite.com", "203.0.113.10", 300),
        SeedRecord::new("cdn2.mysite.com", "203.0.113.20", 300),
        SeedRecord::new("edge.mysite.com", "203.0.113.30", 300),
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
    println!("successful: {}", successful);
    println!("failed: {}", failed);
    println!("total: {}", records.len());

    Ok(())
}
