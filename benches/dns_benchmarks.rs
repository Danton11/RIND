use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::net::Ipv4Addr;

use rind::packet::{parse, build_response, DnsQuery, Question};
use rind::update::{DnsRecord, DnsRecords, load_records_from_file, save_records_to_file};
use tempfile::NamedTempFile;
use std::io::Write;

fn create_test_packet() -> Vec<u8> {
    vec![
        0x12, 0x34, // ID
        0x01, 0x00, // Flags
        0x00, 0x01, // QDCOUNT
        0x00, 0x00, // ANCOUNT
        0x00, 0x00, // NSCOUNT
        0x00, 0x00, // ARCOUNT
        4, b't', b'e', b's', b't', 3, b'c', b'o', b'm', 0, // test.com
        0x00, 0x01, // QTYPE: A
        0x00, 0x01  // QCLASS: IN
    ]
}

fn create_test_query() -> DnsQuery {
    DnsQuery {
        id: 0x1234,
        flags: 0x0100,
        questions: vec![Question {
            name: "test.com".to_string(),
            qtype: 1,
            qclass: 1,
        }],
        has_opt: false,
        opt_payload_size: 512,
    }
}

fn bench_packet_parsing(c: &mut Criterion) {
    let packet = create_test_packet();
    
    c.bench_function("parse_dns_packet", |b| {
        b.iter(|| {
            let result = parse(black_box(&packet));
            black_box(result)
        })
    });
}

fn bench_response_building(c: &mut Criterion) {
    let query = create_test_query();
    let ip = Ipv4Addr::new(192, 168, 1, 1);
    
    c.bench_function("build_dns_response", |b| {
        b.iter(|| {
            let response = build_response(
                black_box(query.clone()),
                black_box(ip),
                black_box(0),
                black_box(300),
                black_box("A".to_string()),
                black_box("IN".to_string())
            );
            black_box(response)
        })
    });
}

fn bench_record_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("record_operations");
    
    // Benchmark file loading with different record counts
    for record_count in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("load_records", record_count),
            record_count,
            |b, &count| {
                // Create temp file with records
                let mut file = NamedTempFile::new().unwrap();
                for i in 0..count {
                    writeln!(file, "test{}.com:192.168.1.{}:300:A:IN", i, i % 255 + 1).unwrap();
                }
                let path = file.path().to_str().unwrap();
                
                b.iter(|| {
                    let records = load_records_from_file(black_box(path));
                    black_box(records)
                });
            },
        );
    }
    
    // Benchmark file saving
    group.bench_function("save_records", |b| {
        let mut records = DnsRecords::new();
        for i in 0..100 {
            records.insert(format!("test{}.com", i), DnsRecord {
                name: format!("test{}.com", i),
                ip: Some(Ipv4Addr::new(192, 168, 1, (i % 255) as u8 + 1)),
                ttl: 300,
                record_type: "A".to_string(),
                class: "IN".to_string(),
                value: None,
            });
        }
        
        b.iter(|| {
            let file = NamedTempFile::new().unwrap();
            let path = file.path().to_str().unwrap();
            let result = save_records_to_file(black_box(path), black_box(&records));
            black_box(result)
        });
    });
    
    group.finish();
}

fn bench_concurrent_parsing(c: &mut Criterion) {
    let packets: Vec<Vec<u8>> = (0..100).map(|_| create_test_packet()).collect();
    
    c.bench_function("concurrent_packet_parsing", |b| {
        b.iter(|| {
            let results: Vec<_> = packets.iter().map(|packet| {
                parse(black_box(packet))
            }).collect();
            black_box(results)
        })
    });
}

criterion_group!(
    benches,
    bench_packet_parsing,
    bench_response_building,
    bench_record_operations,
    bench_concurrent_parsing
);
criterion_main!(benches);