//! Benchmarks for EVT3 decoder performance.
//!
//! Run with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use evt3_core::Evt3Decoder;
use std::path::PathBuf;

const TEST_FILE_CANDIDATES: [&str; 2] = ["test_data/laser.raw", "../test_data/laser.raw"];

fn synthetic_bytes() -> Vec<u8> {
    let mut data = Vec::new();

    // Generate 100k synthetic events worth of data.
    for i in 0..100_000 {
        data.extend_from_slice(&0x8000u16.to_le_bytes());
        data.extend_from_slice(&((0x6000 | (i & 0x0FFF)) as u16).to_le_bytes());
        data.extend_from_slice(&((i & 0x07FF) as u16).to_le_bytes());
        data.extend_from_slice(&(0x2800u16 | ((i * 3) & 0x07FF) as u16).to_le_bytes());
    }

    data
}

fn decode_file_benchmark(c: &mut Criterion) {
    let Some(test_path) = TEST_FILE_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
    else {
        eprintln!(
            "Benchmark skipped: test file not found in {:?}",
            TEST_FILE_CANDIDATES
        );
        return;
    };

    // Get file size for throughput calculation
    let file_size = std::fs::metadata(&test_path).unwrap().len();

    let mut group = c.benchmark_group("decode_file");
    group.throughput(Throughput::Bytes(file_size));

    group.bench_function("full_file", |b| {
        b.iter(|| {
            let mut decoder = Evt3Decoder::new();
            let result = decoder.decode_file(black_box(&test_path)).unwrap();
            black_box(result.cd_events.len())
        })
    });

    group.finish();
}

fn decode_buffer_benchmark(c: &mut Criterion) {
    let data = synthetic_bytes();

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

fn decode_bytes_benchmark(c: &mut Criterion) {
    let data = synthetic_bytes();

    let mut group = c.benchmark_group("decode_bytes");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("even_chunks", |b| {
        b.iter(|| {
            let mut decoder = Evt3Decoder::new();
            let mut cd_events = Vec::new();
            let mut trigger_events = Vec::new();

            for chunk in data.chunks(8192) {
                decoder
                    .decode_bytes(black_box(chunk), &mut cd_events, &mut trigger_events)
                    .unwrap();
            }

            decoder.finish_stream().unwrap();
            black_box(cd_events.len())
        })
    });

    group.bench_function("irregular_chunks", |b| {
        b.iter(|| {
            let mut decoder = Evt3Decoder::new();
            let mut cd_events = Vec::new();
            let mut trigger_events = Vec::new();

            for chunk in data.chunks(4095) {
                decoder
                    .decode_bytes(black_box(chunk), &mut cd_events, &mut trigger_events)
                    .unwrap();
            }

            decoder.finish_stream().unwrap();
            black_box(cd_events.len())
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    decode_file_benchmark,
    decode_buffer_benchmark,
    decode_bytes_benchmark
);
criterion_main!(benches);
