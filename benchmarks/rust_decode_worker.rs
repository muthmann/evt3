//! Shared measurement harness. The candidate adapter selects its explicit policy;
//! the pre-optimization adapter uses its original serial APIs.
use evt3::{ColumnarDecodeResult, ColumnarEventSink, DecodeResult, Evt3Decoder};
use std::hint::black_box;
use std::io::{BufRead, Read};
use std::path::PathBuf;
use std::time::Instant;

static THREADS: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();

fn new_decoder() -> Evt3Decoder {
    let decoder = Evt3Decoder::new();
    #[cfg(evt3_parallel_api)]
    if let Some(n) = THREADS.get().copied().flatten() {
        let policy = match n {
            0 => evt3::Parallelism::Auto,
            1 => evt3::Parallelism::Off,
            n => evt3::Parallelism::Threads(n),
        };
        return decoder.with_parallelism(policy);
    }
    decoder
}

fn decode_columns(decoder: &mut Evt3Decoder, bytes: &[u8], sink: &mut ColumnarEventSink) {
    #[cfg(evt3_parallel_api)]
    decoder.decode_bytes_columns(bytes, sink);
    #[cfg(not(evt3_parallel_api))]
    decoder.decode_bytes_into(bytes, sink);
}

#[derive(Default)]
struct Signature {
    count: usize,
    sums: [u64; 4],
    ordered_hash: u64,
    trigger_count: usize,
    trigger_sums: [u64; 3],
}

impl Signature {
    fn cd(&mut self, x: u16, y: u16, p: u8, t: u64) {
        self.count += 1;
        for (sum, value) in self.sums.iter_mut().zip([x as u64, y as u64, p as u64, t]) {
            *sum = sum.wrapping_add(value);
        }
        self.ordered_hash = self.ordered_hash.rotate_left(7)
            ^ (u64::from(x) | (u64::from(y) << 16) | (u64::from(p) << 32))
            ^ t.wrapping_mul(0x9e3779b97f4a7c15);
    }

    fn trigger(&mut self, value: u8, id: u8, t: u64) {
        self.trigger_count += 1;
        for (sum, value) in self
            .trigger_sums
            .iter_mut()
            .zip([value as u64, id as u64, t])
        {
            *sum = sum.wrapping_add(value);
        }
    }

    fn rows(&mut self, result: &DecodeResult) {
        for e in &result.cd_events {
            self.cd(e.x, e.y, e.polarity, e.timestamp);
        }
        for e in &result.trigger_events {
            self.trigger(e.value, e.id, e.timestamp);
        }
    }

    fn columns(&mut self, result: &ColumnarDecodeResult) {
        self.sink_columns(&result.cd_events, &result.trigger_events);
    }

    fn sink_columns(&mut self, cd: &evt3::EventColumns, triggers: &evt3::TriggerColumns) {
        for i in 0..cd.len() {
            self.cd(cd.x[i], cd.y[i], cd.polarity[i], cd.timestamp[i]);
        }
        for i in 0..triggers.len() {
            self.trigger(triggers.value[i], triggers.id[i], triggers.timestamp[i]);
        }
    }

    fn json(&self) -> String {
        format!("{{\"count\":{},\"sums\":{:?},\"ordered_hash\":{},\"trigger_count\":{},\"trigger_sums\":{:?}}}",
            self.count, self.sums, self.ordered_hash, self.trigger_count, self.trigger_sums)
    }
}

enum Output {
    Rows(DecodeResult),
    Columns(ColumnarDecodeResult),
}

fn decode_file(path: &PathBuf, columns: bool) -> Output {
    let mut decoder = new_decoder();
    if columns {
        Output::Columns(decoder.decode_file_columns(path).unwrap())
    } else {
        Output::Rows(decoder.decode_file(path).unwrap())
    }
}

fn files(paths: &[PathBuf], workers: usize, columns: bool) {
    let start = Instant::now();
    let outputs = if workers == 1 {
        paths
            .iter()
            .map(|p| decode_file(p, columns))
            .collect::<Vec<_>>()
    } else {
        std::thread::scope(|scope| {
            let handles: Vec<_> = paths
                .chunks(paths.len().div_ceil(workers))
                .map(|group| {
                    scope.spawn(move || {
                        group
                            .iter()
                            .map(|p| decode_file(p, columns))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap())
                .collect()
        })
    };
    let seconds = start.elapsed().as_secs_f64();
    // Consume every output field after timing, retaining all file results just
    // like the Python full-memory workload does.
    let signatures: Vec<_> = outputs
        .iter()
        .map(|result| {
            let mut signature = Signature::default();
            match result {
                Output::Rows(rows) => signature.rows(rows),
                Output::Columns(columns) => signature.columns(columns),
            }
            signature
        })
        .collect();
    print_result(seconds, &signatures, &[]);
}

fn stream(path: &PathBuf, chunk_bytes: usize, columns: bool) {
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
    let mut rows = Vec::new();
    let mut triggers = Vec::new();
    let mut sink = ColumnarEventSink::default();
    let mut signature = Signature::default();
    let mut measured_count = 0;
    let mut latencies = Vec::with_capacity(data.len().div_ceil(chunk_bytes));
    let mut seconds = 0.0;
    // First verify the exact packet boundaries, then time the same operations
    // with reusable output storage and without a checksum consumer.
    for verify in [true, false] {
        let mut decoder = new_decoder();
        let start = Instant::now();
        for chunk in data.chunks(chunk_bytes) {
            let packet_start = Instant::now();
            let count = if columns {
                sink.clear();
                decode_columns(&mut decoder, black_box(chunk), &mut sink);
                black_box(&sink);
                sink.cd.len()
            } else {
                rows.clear();
                triggers.clear();
                decoder
                    .decode_bytes(black_box(chunk), &mut rows, &mut triggers)
                    .unwrap();
                black_box((&rows, &triggers));
                rows.len()
            };
            if verify {
                if columns {
                    signature.sink_columns(&sink.cd, &sink.triggers);
                } else {
                    for e in &rows {
                        signature.cd(e.x, e.y, e.polarity, e.timestamp);
                    }
                    for e in &triggers {
                        signature.trigger(e.value, e.id, e.timestamp);
                    }
                }
            } else {
                latencies.push(packet_start.elapsed().as_secs_f64());
                measured_count += count;
            }
        }
        decoder.finish_stream().unwrap();
        if !verify {
            seconds = start.elapsed().as_secs_f64();
        }
    }
    assert_eq!(signature.count, measured_count);
    print_result(seconds, &[signature], &latencies);
}

fn print_result(seconds: f64, signatures: &[Signature], latencies: &[f64]) {
    let count: usize = signatures.iter().map(|s| s.count).sum();
    let signatures = signatures
        .iter()
        .map(Signature::json)
        .collect::<Vec<_>>()
        .join(",");
    let mut sorted = latencies.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| {
        if sorted.is_empty() {
            0.0
        } else {
            sorted[((sorted.len() - 1) as f64 * p).ceil() as usize]
        }
    };
    let packets = format!(
        "{{\"count\":{},\"p50\":{},\"p95\":{},\"p99\":{},\"max\":{}}}",
        sorted.len(),
        percentile(0.50),
        percentile(0.95),
        percentile(0.99),
        percentile(1.0)
    );
    println!("{{\"seconds\":{seconds},\"event_count\":{count},\"signature\":[{signatures}],\"packet_latency_seconds\":{packets}}}");
}

fn main() {
    let all_args: Vec<_> = std::env::args().skip(1).collect();
    let (threads, args) = if all_args.first().map(String::as_str) == Some("--threads") {
        (Some(all_args[1].parse::<usize>().unwrap()), &all_args[2..])
    } else {
        (None, &all_args[..])
    };
    THREADS.set(threads).unwrap();
    let size: usize = args[1].parse().unwrap();
    assert!(size > 0);
    let paths: Vec<_> = args[2..].iter().map(PathBuf::from).collect();
    assert!(!paths.is_empty());
    match args[0].as_str() {
        "rows" => files(&paths, size, false),
        "columns" => files(&paths, size, true),
        "stream_rows" => stream(&paths[0], size, false),
        "stream_columns" => stream(&paths[0], size, true),
        mode => panic!("unknown mode: {mode}"),
    }
}
