# EVT3 Decoder Benchmarks

Benchmarks for the evt3 decoder against evt3 0.4 and Prophesee's official
standalone C++ decoder sample.

## Current Results

Apple M1 Pro (8 cores), macOS, Rust 1.95, `laser.raw` (325 MiB,
116,300,447 CD events), median of five runs. Baseline: evt3 0.4
(`8554807`).

| Workflow | evt3 0.4 | Current | Speedup |
|---|---:|---:|---:|
| Python `decode_file`, Auto | 1.110 s | 0.301 s | 3.7x |
| Rust `decode_file` rows, Auto | 0.896 s | 0.293 s | 3.1x |
| Rust `decode_file_columns`, Auto | 0.943 s | 0.277 s | 3.4x |
| Packet replay, 4–64 KiB chunks, one thread | — | — | 1.4–1.8x |
| CLI CSV export | 7.371 s | 6.658 s | 1.11x |
| CLI CSV export vs. C++ sample (11.546 s) | — | 6.658 s | 1.73x |

Auto uses up to four threads, so its gains include extra CPU cores.
Concurrent decoding of several files and some packet sizes miss the 40%
less-time target; the reports list every case.

| Report | Content |
|---|---|
| [final-results-2026-10-25.md](./final-results-2026-10-25.md) | All 22 decode cases and CSV vs. C++, selected implementation |
| [parallelism-controls-2026-10-25.md](./parallelism-controls-2026-10-25.md) | One thread vs. Auto with the explicit controls, packet tail latency |
| [validation-2026-10-25.json](./validation-2026-10-25.json) | Functional, sanitizer, lint, and compiler checks |

Historical reports retain decoder measurements and validation from their
original source snapshots. Application-specific integration evidence has been
removed from the decoder repository; edited JSON reports record this curation
and their original report hashes. No timing samples were changed or rerun.

The whole-file results were measured before the thread controls became
explicit; the parallel algorithm is the same. Re-run the suite before quoting
new numbers. Use `--decoder-threads 1` for one thread, `0` for Auto, or `n`
for a total thread cap, independently of `--workers` (file concurrency).
Omitting it uses each API's default: Off in Rust, Auto in Python and the CLI.

The [scalar comparison](./comparison-results-2026-10-25.md),
[parallel v1 report](./runtime-results-2026-10-25.md), and
[October 24 results](./optimization-results-2026-10-24.md) describe earlier
source snapshots and are kept as history.

## Unified Comparison Suite

One command compares the official Prophesee standalone C++ decoder, a selected
pre-change Rust commit, and the current Rust working tree:

```bash
evt3-python/.venv/bin/python benchmarks/benchmark.py --comparison-suite \
  --rust-baseline-ref 8554807 --iterations 5 \
  --output benchmarks/comparison-results.json
```

Use a Python environment with NumPy and maturin installed, plus Rust and a C++17
compiler. The suite builds both Rust versions in release mode with the same
Python interpreter. The baseline starts from the candidate's dependency
lockfile, refreshes workspace dependency edges offline, and records both lock
hashes. It builds the vendored, unmodified
Prophesee sample with `-O3 -DNDEBUG -std=c++17`. Set `CXX` to choose the compiler,
or pass `--cpp-binary` to use an existing binary whose build settings are then
reported as unknown. Build caches and generated inputs default to
`evt3-benchmark-comparison` in the system temporary directory, away from a
potentially synchronized checkout. `--work-dir /private/tmp/evt3-benchmark-comparison`
selects an explicit location on macOS. Both Rust build targets use this directory.
The selected Rust commit must support the current CLI and Python APIs and be
compatible with the lockfile. Specify the actual pre-change commit when
repeating a comparison after committing an optimization; the default is `HEAD`.

The suite measures these separate workloads:

| Family | Cases | Baselines |
|---|---|---|
| Decode + CSV to the null device | Small file, complete recording, several files sequentially, several files concurrently | Prophesee C++ and pre-change Rust |
| Native Rust rows and columns | The same four cases, without CSV | Pre-change Rust |
| Recorded packet replay | Reusable Rust rows/columns and Python `Decoder.feed`, 4095/8192/65536-byte chunks | Pre-change Rust/Python |
| Python decode to NumPy | The same four file cases | Pre-change Rust/Python |
| Python bounded batches + checksum consumer | Complete recording | Pre-change Rust/Python |

Defaults are an 8 MiB small prefix, the complete `--file` recording, and four
64 MiB prefixes in separate files. These prefixes contain identical recording
data; they exercise separate file decoders, not different camera scenes.
Use `--multi-files a.raw b.raw ...` for independent recordings. `--workers`
controls file concurrency. With explicit opt-in, the current Rust decoder uses
its own pool for large byte chunks; extra CPU parallelism is part of the elapsed
time comparison.
`--small-mib` and `--multi-file-mib` change generated fixture sizes.

Before timing, all three CLIs must produce byte-identical CSV on an 8 MiB
prefix of each input (`--verify-prefix-mib` changes that limit). Python runs
must agree on counts, coordinate/polarity/timestamp sums, and timestamp edges
for every complete decoded file. Each case warms every implementation, rotates
the measured order, and records raw samples, means, medians, ranges, input and
binary hashes, compiler settings, and the exact Rust baseline commit as JSON.
Reports marked `running` contain incomplete results; a successful suite ends
with `status: complete`. Snapshot writes replace the output file atomically.
Python imports and full-memory checksum verification are outside the timer.
Bounded batches include their checksum consumer inside the timer. The native
harness uses a candidate adapter for explicit policy/column APIs, retains full-file
outputs until timing finishes, and then computes counts, sums and an
order-sensitive CD hash. Native packet replay first verifies the same chunk
boundaries outside timing, then reuses output buffers; its timer includes
packet-clock bookkeeping. Python feed input slicing is outside timing, while
NumPy/object creation and retaining all returned arrays are inside. Packet
results record p50/p95/p99/max call latency for each measured run.

Every decode workload has a `runtime_target`: current median time must be at
most 60% of the pre-change median, meaning at least 40% less runtime (1.667x
throughput). CSV timings have their own ratios and no decode-only target.
`all_runtime_targets_passed` evaluates the cases actually measured;
`full_runtime_acceptance_passed` additionally requires every decode case.
`missing_runtime_cases` lists any omissions. `status: complete` only confirms
that the selected measurements finished. Use `--decode-only` to skip CSV timings
while retaining C++ prefix-equality checks.

For focused investigations, select names with shell-style patterns, for example
`--cases 'rust_*/many_parallel' python_full/many_parallel 'rust_stream_*/65536'`.
A passing selection does not establish full acceptance. Run the full suite on
a quiet machine before interpreting performance gates. Packet replay needs
only a recording, so it can run without a connected camera; USB scheduling,
backpressure and end-to-end application latency require a separate hardware test.

Speedup always means `baseline median / current median` within the same
workload. A ratio of `1.25x` means 25% greater throughput or 20% less time.
CSV and Python decode-only times have separate comparisons. The official
standalone sample does not return NumPy arrays, so a C++ ratio for Python
decode-only is explicitly unavailable.

Run the benchmark regression tests with:

```bash
python -m unittest discover -s tests -p test_benchmark.py -v
```

See [optimization-results-2026-10-24.md](./optimization-results-2026-10-24.md)
for the literature-guided byte-conversion and synchronization changes, measured
against the previous Rust decoder with the same compiler.

See [optimization-results-2026-08-12.md](./optimization-results-2026-08-12.md)
for the direct-column, bounded-memory, and compatibility measurements.

## Earlier Results

### August 2026 Optimization Results

**Test file:** [`laser.raw`](https://kdrive.infomaniak.com/app/share/975517/71d66a09-e3b6-480b-ba94-a1509e8ab2c8) (325 MB, 116M events)

| Workflow | Before | Optimized | Improvement |
|---|---:|---:|---:|
| Python `decode_file` | 3.843 s | 2.108 s | **1.82x** |
| Python `decode_file` maximum RSS | 2.879 GB | 1.178 GB | **59.1% less** |
| Streaming CLI with CSV formatting | 15.343 s | 9.185 s | **1.67x** |
| Streaming CLI maximum RSS | 0.940 GB | 0.064 GB | **93.2% less** |

The bounded Python iterator completed the same input in 2.505 s with 0.227 GB
maximum RSS. See the linked results document for exact checksums, methodology,
and interpretation. C++ comparisons remain available through the benchmark
runner, but are not mixed into this before/after optimization table.

### Rust CLI vs. C++ Reference (August 2026)

This comparison gives both implementations the same work: decode the complete
recording, format CSV, and write the bytes to `/dev/null`. The C++ reference
was freshly compiled with Apple Clang 17 using `-O3 -DNDEBUG`. One warm-up per
implementation preceded five alternating measured runs.

| Decoder | Mean time | Median | Range | Events/sec | Speedup |
|---|---:|---:|---:|---:|---:|
| **Rust CLI** | **7.414 s** | 7.390 s | 7.20-7.67 s | **15.69M/s** | **1.62x** |
| C++ reference | 12.028 s | 11.570 s | 11.46-13.33 s | 9.67M/s | 1.00x |

A separate instrumented run measured 63.3 MB maximum RSS for Rust and 28.3 MB
for C++. The Rust implementation wins on elapsed time in this workload; C++
uses less memory. CSV output was byte-identical on an 8-MiB input prefix with
SHA-256 `398d63a52eeb7291caa1346037f674ef0b6209772d3c512e2aa6c70b9aed12f4`.

## Running Benchmarks

### Prerequisites

```bash
# Build Rust release
cargo build --release

# Build C++ reference (optional, for comparison)
c++ -O3 -DNDEBUG -std=c++17 -o cpp_reference/evt3_decoder \
  cpp_reference/metavision_evt3_raw_file_decoder.cpp

# Install Python package
cd evt3-python
uv venv && uv pip install maturin numpy
source .venv/bin/activate && maturin develop --release
```

### Run Python Benchmark

```bash
python benchmarks/benchmark.py --file test_data/laser.raw --iterations 5
```

### Run Like-for-Like Rust/C++ CSV Benchmark

```bash
python benchmarks/benchmark.py --file test_data/laser.raw \
  --csv-comparison-only --iterations 5
```

This mode verifies byte equality on an 8-MiB prefix, warms both binaries, then
alternates their order while both write the complete formatted CSV stream to
the platform null device.

### Run Rust Criterion Benchmarks

```bash
cargo bench
```

Results will be saved to `target/criterion/` with HTML reports.

## Methodology

- Each decoder is run 3 times by default (configurable with `--iterations`)
- **Rust (Python)**: Measures pure decode time (events loaded into memory)
- **Rust (Python batches)**: Measures decode plus bounded batch-object creation;
  batches are released as iteration advances
- **Rust CLI / C++ Reference**: Measures decode + CSV file write (I/O bound)
- Events/sec calculated from average time
- The dedicated CSV comparison alternates Rust and C++ run order and reports
  speedup relative to the C++ reference decoder

Python decode results are released between iterations so the next iteration
does not overlap the previous result's memory. Compare pure decoders with each
other and decode-plus-write tools with each other; the two groups measure
different work.

## Hardware

Results may vary based on:
- CPU (single-threaded performance)
- Disk I/O speed
- Memory bandwidth

All results in this directory were measured on Apple Silicon macOS; the October 2026 results on an Apple M1 Pro with 8 cores.
