use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::net::Ipv4Addr;
use std::sync::Arc;

use rind::packet::{build_response, parse, DnsQuery, Question};
use rind::storage::LmdbStore;
use rind::update::{self, DnsRecord, RecordData};
use tempfile::tempdir;
use tokio::runtime::Runtime;

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

fn seeded_records(count: usize) -> Vec<DnsRecord> {
    (0..count)
        .map(|i| {
            DnsRecord::new(
                format!("test{}.com", i),
                300,
                "IN".to_string(),
                RecordData::A {
                    ip: Ipv4Addr::new(192, 168, 1, (i % 255) as u8 + 1),
                },
            )
        })
        .collect()
}

fn bench_record_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("record_operations");

    // Pre-populate one store per size outside the bench closure. Criterion
    // invokes the `bench_with_input` routine repeatedly during warmup and
    // sample-size calibration, so any `LmdbStore::open` inside the closure
    // accumulates file descriptors across runs and eventually trips EMFILE
    // on the larger sizes. Setup once, capture by reference.
    let read_setups: Vec<(usize, tempfile::TempDir, LmdbStore)> = [10, 100, 1000]
        .iter()
        .map(|&count| {
            let dir = tempdir().unwrap();
            let store = LmdbStore::open(dir.path()).unwrap();
            for record in seeded_records(count) {
                store.put_record(&record).unwrap();
            }
            (count, dir, store)
        })
        .collect();

    for (count, _dir, store) in &read_setups {
        group.bench_with_input(
            BenchmarkId::new("list_all_records", count),
            count,
            |b, _| {
                b.iter(|| black_box(store.list_all_records()));
            },
        );
    }

    // Legacy batched-insert bench: 100 fresh records per iteration, all with
    // unique ids, so every call hits the `existed == false` branch. Kept as a
    // rough top-line number that rolls up per-iter commit cost × 100.
    group.bench_function("put_record_batch_100", |b| {
        let dir = tempdir().unwrap();
        let store = LmdbStore::open(dir.path()).unwrap();
        let mut generation = 0u64;
        b.iter(|| {
            generation += 1;
            for i in 0..100u32 {
                let record = DnsRecord::new(
                    format!("bench-{}-{}.test", generation, i),
                    300,
                    "IN".to_string(),
                    RecordData::A {
                        ip: Ipv4Addr::new(10, 0, (i / 256) as u8, (i % 256) as u8),
                    },
                );
                store.put_record(black_box(&record)).unwrap();
            }
        });
    });

    // Single insert on an empty-ish env. `existed == false` branch only.
    // Each iter gets a fresh DnsRecord via `iter_batched` so the generator
    // cost is excluded from the measured routine. The store still grows
    // across iters (ids are distinct), so this trends toward the "insert
    // into a growing btree" cost — fine for a baseline, see the scaling
    // bench below if the trend actually matters.
    group.bench_function("put_record_insert_new", |b| {
        let dir = tempdir().unwrap();
        let store = LmdbStore::open(dir.path()).unwrap();
        let mut counter = 0u64;
        b.iter_batched(
            || {
                counter += 1;
                DnsRecord::new(
                    format!("insert-{}.test", counter),
                    300,
                    "IN".to_string(),
                    RecordData::A {
                        ip: Ipv4Addr::new(10, 0, 0, 1),
                    },
                )
            },
            |record| store.put_record(black_box(&record)).unwrap(),
            BatchSize::SmallInput,
        );
    });

    // Replace an existing record — exercises the `existed == true` branch:
    // reads old bytes, XORs old hash out, deletes old secondary-index entry,
    // then the four writes of the normal insert. Should cost ~1.5–2× the
    // insert case. Reuses the same id every iter.
    group.bench_function("put_record_replace_existing", |b| {
        let dir = tempdir().unwrap();
        let store = LmdbStore::open(dir.path()).unwrap();
        // Seed one record; every iter overwrites this exact id.
        let seed = DnsRecord::new(
            "replace.test".to_string(),
            300,
            "IN".to_string(),
            RecordData::A {
                ip: Ipv4Addr::new(10, 0, 0, 1),
            },
        );
        let id = seed.id.clone();
        store.put_record(&seed).unwrap();
        let mut counter = 0u32;
        b.iter_batched(
            || {
                counter = counter.wrapping_add(1);
                // Same id, varying rdata so the serialized value isn't
                // byte-identical (forces the hash XOR path to do real work).
                DnsRecord {
                    id: id.clone(),
                    name: "replace.test".to_string(),
                    ttl: 300,
                    class: "IN".to_string(),
                    data: RecordData::A {
                        ip: Ipv4Addr::new(10, 0, (counter >> 8) as u8, counter as u8),
                    },
                    created_at: seed.created_at,
                    updated_at: chrono::Utc::now(),
                }
            },
            |record| store.put_record(black_box(&record)).unwrap(),
            BatchSize::SmallInput,
        );
    });

    // Delete. Setup per iter: seed a record, return its id; timed routine
    // is just the delete call. `iter_batched` charges setup time separately
    // so the create-then-delete pair isn't conflated.
    group.bench_function("delete_record", |b| {
        let dir = tempdir().unwrap();
        let store = LmdbStore::open(dir.path()).unwrap();
        let mut counter = 0u64;
        b.iter_batched(
            || {
                counter += 1;
                let record = DnsRecord::new(
                    format!("del-{}.test", counter),
                    300,
                    "IN".to_string(),
                    RecordData::A {
                        ip: Ipv4Addr::new(10, 0, 0, 1),
                    },
                );
                let id = record.id.clone();
                store.put_record(&record).unwrap();
                id
            },
            |id| {
                store.delete_record_by_id(black_box(&id)).unwrap();
            },
            BatchSize::SmallInput,
        );
    });

    // End-to-end `update::create_record` — the path every REST POST actually
    // takes. Includes the read-txn `find_records_by_name` conflict scan plus
    // the put_record write txn. The gap between this and `put_record_insert_new`
    // is the cost of the conflict check.
    group.bench_function("create_record_end_to_end", |b| {
        let rt = Runtime::new().unwrap();
        let dir = tempdir().unwrap();
        let store = Arc::new(LmdbStore::open(dir.path()).unwrap());
        let mut counter = 0u64;
        b.iter(|| {
            counter += 1;
            let name = format!("e2e-{}.test", counter);
            rt.block_on(async {
                update::create_record(
                    Arc::clone(&store),
                    name,
                    300,
                    "IN".to_string(),
                    RecordData::A {
                        ip: Ipv4Addr::new(10, 0, 0, 1),
                    },
                    None,
                )
                .await
                .unwrap();
            });
        });
    });

    // Batched commit — N inserts amortized over one fsync. On real disk,
    // commit is ~95% of a single put and ~60% of that is fdatasync; if the
    // amortization story is real, this should drop per-record cost by ~N
    // until we hit a non-fsync ceiling.
    for &batch_size in &[10usize, 100, 1000] {
        group.bench_function(
            BenchmarkId::new("put_records_batch_commit_once", batch_size),
            |b| {
                let dir = tempdir().unwrap();
                let store = LmdbStore::open(dir.path()).unwrap();
                let mut generation = 0u64;
                b.iter_batched(
                    || {
                        generation += 1;
                        (0..batch_size)
                            .map(|i| {
                                DnsRecord::new(
                                    format!("batch-{}-{}.test", generation, i),
                                    300,
                                    "IN".to_string(),
                                    RecordData::A {
                                        ip: Ipv4Addr::new(10, 1, (i / 256) as u8, (i % 256) as u8),
                                    },
                                )
                            })
                            .collect::<Vec<_>>()
                    },
                    |records| store.put_records_batch(black_box(&records)).unwrap(),
                    BatchSize::SmallInput,
                );
            },
        );
    }

    // Scaling: single insert into a pre-seeded env of N records. The btree
    // depth grows as log(N) so insert cost should trend slowly upward. If
    // the gap between N=0 and N=10k is >2×, something else is going on
    // (page split storms, fsync stalls).
    //
    // Seed cost lives outside the bench, so the measured column is "single
    // insert with N records already present".
    for &seed_count in &[0usize, 1_000, 10_000] {
        let dir = tempdir().unwrap();
        let store = LmdbStore::open(dir.path()).unwrap();
        for record in seeded_records(seed_count) {
            store.put_record(&record).unwrap();
        }
        let mut counter = 0u64;
        group.bench_with_input(
            BenchmarkId::new("put_record_at_size", seed_count),
            &seed_count,
            |b, _| {
                b.iter_batched(
                    || {
                        counter += 1;
                        DnsRecord::new(
                            format!("probe-{}.test", counter),
                            300,
                            "IN".to_string(),
                            RecordData::A {
                                ip: Ipv4Addr::new(10, 0, 0, 1),
                            },
                        )
                    },
                    |record| store.put_record(black_box(&record)).unwrap(),
                    BatchSize::SmallInput,
                );
            },
        );
        // `dir` and `store` drop at end of loop iteration — the bench above
        // ran synchronously, no lingering references.
    }

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
