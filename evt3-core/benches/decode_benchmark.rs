//! Benchmarks for EVT3 decoder performance.
//!
//! Run with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use evt3::{ColumnarEventSink, EventSink, Evt3Decoder};
use std::io::{BufRead, Read};
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

fn synthetic_vector_bytes() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0x8000u16.to_le_bytes());
    for i in 0..100_000_u32 {
        data.extend_from_slice(&(0x6000_u16 | (i & 0x0fff) as u16).to_le_bytes());
        data.extend_from_slice(&((i & 0x07ff) as u16).to_le_bytes());
        data.extend_from_slice(&(0x3000_u16 | ((i * 3) & 0x07ff) as u16).to_le_bytes());
        data.extend_from_slice(&(0x4001_u16 | ((i & 0x0f) << 4) as u16).to_le_bytes());
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

    group.bench_function("full_file_columns", |b| {
        b.iter(|| {
            let result = Evt3Decoder::new()
                .decode_file_columns(black_box(&test_path))
                .unwrap();
            black_box(result.cd_events.len())
        })
    });

    group.finish();
}

fn decode_buffer_benchmark(c: &mut Criterion) {
    let data = synthetic_bytes();

    // Matches the decoder, which keeps `chunks_exact` for the 1.83 MSRV.
    #[allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]
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

fn decode_vector_benchmark(c: &mut Criterion) {
    let data = synthetic_vector_bytes();
    let mut group = c.benchmark_group("decode_vectors");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("sparse_vector_masks", |b| {
        b.iter(|| {
            let mut decoder = Evt3Decoder::new();
            let mut sink = ColumnarEventSink::default();
            decoder.decode_bytes_into(black_box(&data), &mut sink);
            black_box(sink.cd.len())
        })
    });
    group.finish();
}

fn decode_vector_density_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode_vector_density");
    for (name, masks) in [
        ("empty", &[0x000, 0x000][..]),
        ("sparse", &[0x001, 0x800, 0x040, 0x008][..]),
        ("dense", &[0xfff, 0x7ff, 0xffe, 0xfef][..]),
        ("varying", &[0x000, 0x001, 0xaaa, 0xfff, 0x800, 0x135][..]),
    ] {
        let mut data = Vec::new();
        data.extend_from_slice(&0x8000_u16.to_le_bytes());
        let mut event_count = 0;
        for i in 0..100_000 {
            let mask = masks[i % masks.len()];
            for word in [
                0x6000 | (i & 0x0fff) as u16,
                (i % 720) as u16,
                0x3000 | (i % 1200) as u16,
                0x4000 | mask,
            ] {
                data.extend_from_slice(&word.to_le_bytes());
            }
            event_count += mask.count_ones() as usize;
        }
        group.throughput(Throughput::Bytes(data.len() as u64));

        // Reuse storage to isolate the decode loop from allocation cost.
        let mut columns = ColumnarEventSink::with_cd_capacity(event_count);
        group.bench_with_input(BenchmarkId::new("columns", name), &data, |b, data| {
            b.iter(|| {
                columns.clear();
                Evt3Decoder::new().decode_bytes_into(black_box(data), &mut columns);
                black_box(&columns);
            })
        });
        let mut cd = Vec::with_capacity(event_count);
        let mut triggers = Vec::new();
        group.bench_with_input(BenchmarkId::new("rows", name), &data, |b, data| {
            b.iter(|| {
                cd.clear();
                triggers.clear();
                Evt3Decoder::new()
                    .decode_bytes(black_box(data), &mut cd, &mut triggers)
                    .unwrap();
                black_box((&cd, &triggers));
            })
        });
    }
    group.finish();
}

#[derive(Default)]
struct ChecksumSink {
    events: usize,
    triggers: usize,
    checksum: u64,
}

impl EventSink for ChecksumSink {
    fn cd_len(&self) -> usize {
        self.events
    }
    fn trigger_len(&self) -> usize {
        self.triggers
    }
    fn push_cd(&mut self, x: u16, y: u16, polarity: u8, timestamp: u64) {
        self.events += 1;
        self.checksum = self
            .checksum
            .wrapping_add(u64::from(x) + u64::from(y) + u64::from(polarity) + timestamp);
    }
    fn push_trigger(&mut self, value: u8, id: u8, timestamp: u64) {
        self.triggers += 1;
        self.checksum = self
            .checksum
            .wrapping_add(u64::from(value) + u64::from(id) + timestamp);
    }
}

fn real_stream_benchmark(c: &mut Criterion) {
    let Some(path) = TEST_FILE_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
    else {
        return;
    };
    let mut reader = std::io::BufReader::new(std::fs::File::open(path).unwrap());
    while reader.fill_buf().unwrap().first() == Some(&b'%') {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line.starts_with("% end") {
            break;
        }
    }
    let mut data = Vec::new();
    reader.take(8 << 20).read_to_end(&mut data).unwrap();
    data.truncate(data.len() & !1);
    let mut group = c.benchmark_group("real_stream");
    group.throughput(Throughput::Bytes(data.len() as u64));
    for size in [4095, 8192, 65536] {
        let mut rows = Vec::new();
        let mut triggers = Vec::new();
        group.bench_with_input(BenchmarkId::new("rows", size), &size, |b, &size| {
            b.iter(|| {
                let mut decoder = Evt3Decoder::new();
                for bytes in data.chunks(size) {
                    rows.clear();
                    triggers.clear();
                    decoder
                        .decode_bytes(black_box(bytes), &mut rows, &mut triggers)
                        .unwrap();
                    black_box((&rows, &triggers));
                }
                decoder.finish_stream().unwrap();
            });
        });
        let mut columns = ColumnarEventSink::default();
        group.bench_with_input(BenchmarkId::new("columns", size), &size, |b, &size| {
            b.iter(|| {
                let mut decoder = Evt3Decoder::new();
                for bytes in data.chunks(size) {
                    columns.clear();
                    decoder.decode_bytes_into(black_box(bytes), &mut columns);
                    black_box(&columns);
                }
                decoder.finish_stream().unwrap();
            });
        });
        group.bench_with_input(BenchmarkId::new("checksum", size), &size, |b, &size| {
            b.iter(|| {
                let mut decoder = Evt3Decoder::new();
                let mut sink = ChecksumSink::default();
                for bytes in data.chunks(size) {
                    decoder.decode_bytes_into(black_box(bytes), &mut sink);
                    black_box(&sink);
                }
                decoder.finish_stream().unwrap();
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    decode_file_benchmark,
    decode_buffer_benchmark,
    decode_bytes_benchmark,
    decode_vector_benchmark,
    decode_vector_density_benchmark,
    real_stream_benchmark
);
criterion_main!(benches);
