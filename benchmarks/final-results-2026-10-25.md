# Final selected decoder measurements, 25 October 2026

**Historical source snapshot:** this run predates explicit parallelism controls.
Rust now defaults to Off, and generic sinks always run serially. These numbers
do not describe the new defaults. See the
[parallelism control checks](parallelism-controls-2026-10-25.md).

Optimization ended at the user's request with the best available implementation.
This report contains one complete five-sample comparison of that selected source
snapshot, performed after the user confirmed that the machine was sufficiently free.
It does not select the fastest individual samples or combine results from different candidates.

**Original target: 13/22 decode cases reach at least 40% less elapsed time.**
Full runtime acceptance: **false**.
The requested optimization work is finished; the original target is not treated as universally met.

Original Rust: `85548072841ee92de409206883f4c130644ad4fe`.
The [raw JSON](final-results-2026-10-25.json) records every sample, input hash,
source hash, binary/wheel hash, dependency lock and build setting.
Rust 1.95.0, CPython 3.13.5, macOS arm64, eight logical CPUs; warm filesystem cache.
Each implementation receives one warmup; measured order rotates across five runs.
Both Rust revisions use the same toolchain and unchanged native harness.

## Files and Python

These times include file reading, decoding and output allocation. Full outputs remain
alive until timing ends; verification follows. Python imports are excluded.
The bounded Python batch case includes its checksum consumer.

| Workload | Original (ms) | Selected (ms) | Throughput ratio | Less time | 40% target |
|---|---:|---:|---:|---:|---|
| `rust_rows/small` | 22.006 | 7.352 | 2.993x | 66.6% | Pass |
| `rust_rows/large` | 895.463 | 293.425 | 3.052x | 67.2% | Pass |
| `rust_rows/many_sequential` | 734.354 | 231.372 | 3.174x | 68.5% | Pass |
| `rust_rows/many_parallel` | 192.884 | 124.422 | 1.550x | 35.5% | Miss |
| `rust_columns/small` | 22.515 | 6.474 | 3.478x | 71.2% | Pass |
| `rust_columns/large` | 943.201 | 276.546 | 3.411x | 70.7% | Pass |
| `rust_columns/many_sequential` | 776.297 | 249.792 | 3.108x | 67.8% | Pass |
| `rust_columns/many_parallel` | 284.863 | 181.855 | 1.566x | 36.2% | Miss |
| `python_full/small` | 24.973 | 8.300 | 3.009x | 66.8% | Pass |
| `python_full/large` | 1109.666 | 300.556 | 3.692x | 72.9% | Pass |
| `python_full/many_sequential` | 774.595 | 223.238 | 3.470x | 71.2% | Pass |
| `python_full/many_parallel` | 213.683 | 128.945 | 1.657x | 39.7% | Miss |
| `python_batches/large` | 1249.545 | 336.559 | 3.713x | 73.1% | Pass |

Small = one 8-MiB prefix. Large = the complete 341,306,299-byte recording
(325.495 MiB, 116,300,447 CD events). Multiple files = four separate 64-MiB
prefixes of the same recording, sequentially or with four file workers.
These are repeated scene data, not four independent camera scenes.
The selected decoder also uses a shared background pool, so gains include extra CPU parallelism.

## Recorded packet replay

Each value below is total time for an 8-MiB payload replay, **not time for one packet**.
Input is already in memory. Native output buffers are reused; Python `Decoder.feed`
includes NumPy/object creation and retention of every returned array.
The timer includes per-call clock bookkeeping. Chunk sizes are bytes; 4095 exercises odd boundaries.

| API / chunk bytes | Original (ms) | Selected (ms) | Throughput ratio | Less time | 40% target |
|---|---:|---:|---:|---:|---|
| `rust_stream_rows/4095` | 19.970 | 14.774 | 1.352x | 26.0% | Miss |
| `rust_stream_columns/4095` | 22.487 | 13.622 | 1.651x | 39.4% | Miss |
| `python_feed/4095` | 34.271 | 18.995 | 1.804x | 44.6% | Pass |
| `rust_stream_rows/8192` | 18.672 | 12.928 | 1.444x | 30.8% | Miss |
| `rust_stream_columns/8192` | 25.836 | 15.859 | 1.629x | 38.6% | Miss |
| `python_feed/8192` | 32.155 | 22.172 | 1.450x | 31.0% | Miss |
| `rust_stream_rows/65536` | 18.507 | 11.523 | 1.606x | 37.7% | Miss |
| `rust_stream_columns/65536` | 19.253 | 9.510 | 2.025x | 50.6% | Pass |
| `python_feed/65536` | 32.733 | 14.783 | 2.214x | 54.8% | Pass |

Per-call latency in microseconds: each p50/p95/p99 is the median of the five
per-run statistics. Worst call is the maximum across all measured runs.
Quantiles describe synchronous decoding only, excluding USB transfer and downstream processing.

| API / chunk bytes | Original p50 / p95 / p99 (us) | Selected p50 / p95 / p99 (us) | Worst call original / selected (us) |
|---|---:|---:|---:|
| `rust_stream_rows/4095` | 9.00 / 10.00 / 28.71 | 5.92 / 11.50 / 21.50 | 382.92 / 12143.25 |
| `rust_stream_columns/4095` | 9.83 / 11.83 / 31.12 | 5.88 / 7.29 / 23.46 | 845.88 / 735.38 |
| `python_feed/4095` | 15.04 / 20.04 / 61.62 | 8.42 / 11.42 / 27.96 | 244.37 / 516.33 |
| `rust_stream_rows/8192` | 17.38 / 19.58 / 44.54 | 11.62 / 13.17 / 31.29 | 30231.17 / 6475.96 |
| `rust_stream_columns/8192` | 19.50 / 36.62 / 126.92 | 11.75 / 24.21 / 88.79 | 30100.17 / 1972.50 |
| `python_feed/8192` | 27.96 / 40.21 / 94.58 | 16.62 / 32.67 / 90.12 | 929.63 / 1293.50 |
| `rust_stream_rows/65536` | 140.71 / 171.67 / 210.92 | 81.50 / 118.33 / 158.79 | 730.75 / 751.29 |
| `rust_stream_columns/65536` | 147.67 / 168.83 / 206.92 | 69.67 / 102.17 / 119.92 | 11563.92 / 150.46 |
| `python_feed/65536` | 207.42 / 350.46 / 742.33 | 100.42 / 216.00 / 367.04 | 2503.58 / 12503.92 |

## Official Prophesee C++ comparison

All three implementations perform the same CSV-export workflow to the null device.
This includes process startup, reading, decoding and CSV formatting.
The unmodified [official standalone sample](https://github.com/prophesee-ai/openeb/blob/main/standalone_samples/metavision_evt3_raw_file_decoder/metavision_evt3_raw_file_decoder.cpp)
is compiled with Apple Clang 21, `-O3 -DNDEBUG -std=c++17`.
These figures do not measure every Metavision SDK mode.
The standalone sample has no equivalent packet/NumPy output API, so no C++
decode-only or Python ratio is inferred from CSV timings.

| CSV workload | C++ (s) | Original Rust (s) | Selected Rust (s) | vs C++ | Less time vs C++ | vs original Rust | Less time vs original |
|---|---:|---:|---:|---:|---:|---:|---:|
| `csv/small` | 0.277073 | 0.178198 | 0.158558 | 1.747x | 42.8% | 1.124x | 11.0% |
| `csv/large` | 11.546487 | 7.370735 | 6.658449 | 1.734x | 42.3% | 1.107x | 9.7% |
| `csv/many_sequential` | 9.106570 | 5.534838 | 5.050768 | 1.803x | 44.5% | 1.096x | 8.7% |
| `csv/many_parallel` | 2.363016 | 1.592650 | 1.514951 | 1.560x | 35.9% | 1.051x | 4.9% |

## Timing variation

The following cases have a largest/smallest sample ratio of at least 1.25
in one implementation. Ranges include every measured sample, without trimming.
Median target classifications are descriptive; a narrow pass near 40% does
not establish a robust margin under this variation.

| Workload | Original Rust min–max (ms) | Selected min–max (ms) |
|---|---:|---:|
| `csv/many_parallel` | 1439.478–2056.269 | 1339.575–2458.407 |
| `rust_rows/small` | 21.586–30.688 | 6.523–9.053 |
| `rust_rows/many_sequential` | 700.959–1222.070 | 225.574–237.779 |
| `rust_columns/many_sequential` | 732.047–1100.706 | 226.047–266.247 |
| `rust_columns/many_parallel` | 257.830–1507.177 | 158.712–359.421 |
| `rust_stream_rows/4095` | 18.661–24.767 | 12.605–39.331 |
| `rust_stream_columns/4095` | 20.130–24.010 | 12.914–16.302 |
| `rust_stream_rows/8192` | 17.720–111.498 | 11.778–31.620 |
| `rust_stream_columns/8192` | 19.335–200.443 | 11.605–18.506 |
| `python_feed/8192` | 28.922–36.572 | 17.429–23.536 |
| `rust_stream_rows/65536` | 18.118–19.422 | 9.871–12.967 |
| `rust_stream_columns/65536` | 19.064–33.877 | 8.941–10.311 |
| `python_feed/65536` | 25.019–36.391 | 12.507–108.828 |
| `python_full/small` | 23.829–26.853 | 7.189–10.419 |
| `python_full/large` | 998.261–1295.775 | 269.701–376.844 |
| `python_full/many_parallel` | 205.468–298.228 | 127.491–168.134 |

## Remaining limits and validation

The 40%-less-time rule means `selected / original <= 0.60`, equivalent to
at least 1.667x throughput. It applies to the 22 decode cases, not CSV export.
Cases below the threshold in this complete run:

- `rust_rows/many_parallel`: 35.5% less time, 1.550x throughput.
- `rust_columns/many_parallel`: 36.2% less time, 1.566x throughput.
- `rust_stream_rows/4095`: 26.0% less time, 1.352x throughput.
- `rust_stream_columns/4095`: 39.4% less time, 1.651x throughput.
- `rust_stream_rows/8192`: 30.8% less time, 1.444x throughput.
- `rust_stream_columns/8192`: 38.6% less time, 1.629x throughput.
- `python_feed/8192`: 31.0% less time, 1.450x throughput.
- `rust_stream_rows/65536`: 37.7% less time, 1.606x throughput.
- `python_full/many_parallel`: 39.7% less time, 1.657x throughput.

Five-sample medians are measurements on this machine and recording, not confidence
bounds or guarantees for other scenes, CPUs or system loads. The raw JSON retains
all timing ranges and outliers. Packet tail latency can improve by a different
amount than total replay time; neither is a hard real-time guarantee.

CSV byte equality was checked on an 8-MiB prefix of every input against both
baselines. Native complete outputs additionally match event/trigger counts and
sums plus an order-sensitive CD hash. Python checks counts, sums and timestamp
edges; this is weaker than an element-by-element array comparison.

Final functional checks: 43 Rust unit tests with HDF5, 15 real-file/integration
tests and two documentation tests; 41 unit tests under AddressSanitizer;
23 Python tests (three free-threaded-only tests skipped); 16 benchmark tests.
Clippy with HDF5/all targets, Rust 1.83 Python compilation, formatting and diff checks pass.
The old standalone performance test was explicitly skipped in correctness testing;
this unified suite supplies the performance measurements.
See the [validation snapshot](validation-2026-10-25.json).

These are historical checks against the measured source snapshot, not a new
validation run after documentation curation.

No physical camera was connected. USB scheduling, backpressure, end-to-end
application latency and HDF5 speedups are not established by this replay benchmark.

## Source snapshot and reproduction

Prophesee C++ source SHA-256: `0038d3a0c69b80bea8dc350b3c295592aed92e6ace5030c16c35da6b6c1bb7d0`.
Selected source SHA-256 values (the JSON contains the complete map):

- `evt3-core/src/decoder.rs`: `953cd89a0472d1dd9ed2dbf3458531a131d86de47871eb04ef969f4860e10679`
- `evt3-core/src/decoder/parallel.rs`: `aeef0420bd2cf21660892c05d7b45271f231cae72dd00390315e803542051f5f`
- `evt3-core/src/sink.rs`: `542d17b9a30c64049ec50c9a6af89a36fd38d9011d6cd97706caf12a4affc7c8`
- `evt3-core/src/types.rs`: `9ab1b2c6125d1df780db86f73c53b7852dd105d1e3b375c7a1f27aef3bf0a294`

```bash
CARGO_BUILD_JOBS=4 evt3-python/.venv/bin/python benchmarks/benchmark.py \
  --comparison-suite --file test_data/laser.raw \
  --rust-baseline-ref 8554807 --iterations 5 \
  --work-dir /private/tmp/evt3-benchmark-comparison \
  --output benchmarks/final-results-2026-10-25.json
```

The [research note](literature-and-runtime-2026-10-25.md) links the primary
literature and records accepted and removed experiments. Published speedups
for other encodings are not used as EVT3 speedup claims.
