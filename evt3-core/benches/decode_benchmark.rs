//! Benchmarks for EVT3 decoder performance.
//!
//! Run with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use evt3_core::Evt3Decoder;
use std::path::Path;

const TEST_FILE: &str = "test_data/laser.raw";

fn decode_file_benchmark(c: &mut Criterion) {
    let test_path = Path::new(TEST_FILE);
    if !test_path.exists() {
        eprintln!("Benchmark skipped: test file not found at {}", TEST_FILE);
        return;
    }

    // Get file size for throughput calculation
    let file_size = std::fs::metadata(test_path).unwrap().len();

    let mut group = c.benchmark_group("decode_file");
    group.throughput(Throughput::Bytes(file_size));

    group.bench_function("full_file", |b| {
        b.iter(|| {
            let mut decoder = Evt3Decoder::new();
            let result = decoder.decode_file(black_box(test_path)).unwrap();
            black_box(result.cd_events.len())
        })
    });

    group.finish();
}

fn decode_buffer_benchmark(c: &mut Criterion) {
    // Create synthetic data for buffer decoding
    let mut data = Vec::new();

    // Generate 1M synthetic events worth of data
    for i in 0..100_000 {
        // TIME_HIGH
        data.extend_from_slice(&0x8000u16.to_le_bytes());
        // TIME_LOW
        data.extend_from_slice(&((0x6000 | (i & 0xFFF)) as u16).to_le_bytes());
        // ADDR_Y
        data.extend_from_slice(&((i & 0x7FF) as u16).to_le_bytes());
        // ADDR_X with polarity
        data.extend_from_slice(&(0x2800u16 | ((i * 3) & 0x7FF) as u16).to_le_bytes());
    }

    let words: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();

    let mut group = c.benchmark_group("decode_buffer");
    group.throughput(Throughput::Elements(words.len() as u64));

    group.bench_function("synthetic_100k_events", |b| {
        b.iter(|| {
            let mut decoder = Evt3Decoder::new();
            let mut cd_events = Vec::new();
            let mut trigger_events = Vec::new();
            decoder.decode_buffer(black_box(&words), &mut cd_events, &mut trigger_events);
            black_box(cd_events.len())
        })
    });

    group.finish();
}

criterion_group!(benches, decode_file_benchmark, decode_buffer_benchmark);
criterion_main!(benches);
