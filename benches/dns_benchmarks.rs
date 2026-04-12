use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::net::Ipv4Addr;

use rind::packet::{build_response, parse, DnsQuery, Question};
use rind::update::{
    load_records_from_file, save_records_to_file, DnsRecord, DnsRecords, RecordData,
};
use tempfile::NamedTempFile;

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
        0x00, 0x01, // QCLASS: IN
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
    let data = RecordData::A {
        ip: Ipv4Addr::new(192, 168, 1, 1),
    };

    c.bench_function("build_dns_response", |b| {
        b.iter(|| {
            let answers: &[(&RecordData, u32)] = &[(&data, 300)];
            let response =
                build_response(black_box(query.clone()), black_box(answers), black_box(0));
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
                // Seed a JSONL file via the canonical writer so the format stays in sync.
                let mut seeded = DnsRecords::new();
                for i in 0..count {
                    let record = DnsRecord::new(
                        format!("test{}.com", i),
                        300,
                        "IN".to_string(),
                        RecordData::A {
                            ip: Ipv4Addr::new(192, 168, 1, (i % 255) as u8 + 1),
                        },
                    );
                    seeded.insert(record.id.clone(), record);
                }
                let file = NamedTempFile::new().unwrap();
                let path = file.path().to_str().unwrap().to_string();
                save_records_to_file(&path, &seeded).unwrap();

                b.iter(|| {
                    let records = load_records_from_file(black_box(&path));
                    black_box(records)
                });
            },
        );
    }

    // Benchmark file saving
    group.bench_function("save_records", |b| {
        let mut records = DnsRecords::new();
        for i in 0..100 {
            let record = DnsRecord::new(
                format!("test{}.com", i),
                300,
                "IN".to_string(),
                RecordData::A {
                    ip: Ipv4Addr::new(192, 168, 1, (i % 255) as u8 + 1),
                },
            );
            records.insert(record.id.clone(), record);
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
            let results: Vec<_> = packets
                .iter()
                .map(|packet| parse(black_box(packet)))
                .collect();
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
