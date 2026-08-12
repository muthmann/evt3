# EVT3 Decoder Benchmarks

Performance benchmarks comparing the Rust EVT3 decoder against the C++ reference implementation.

See [optimization-results-2026-08-12.md](./optimization-results-2026-08-12.md)
for the direct-column, bounded-memory, and compatibility measurements.

## Latest Optimization Results

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

## Running Benchmarks

### Prerequisites

```bash
# Build Rust release
cargo build --release

# Build C++ reference (optional, for comparison)
g++ -O2 -o cpp_reference/evt3_decoder cpp_reference/metavision_evt3_raw_file_decoder.cpp

# Install Python package
cd evt3-python
uv venv && uv pip install maturin numpy
source .venv/bin/activate && maturin develop --release
```

### Run Python Benchmark

```bash
python benchmarks/benchmark.py --file test_data/laser.raw --iterations 5
```

### Run Rust Criterion Benchmarks

```bash
cargo bench
```

Results will be saved to `target/criterion/` with HTML reports.

## Methodology

- Each decoder is run 3 times (configurable with `--iterations`)
- **Rust (Python)**: Measures pure decode time (events loaded into memory)
- **Rust (Python batches)**: Measures decode plus bounded batch-object creation;
  batches are released as iteration advances
- **Rust CLI / C++ Reference**: Measures decode + CSV file write (I/O bound)
- Events/sec calculated from average time
- Speedup relative to C++ reference decoder

Python decode results are released between iterations so the next iteration
does not overlap the previous result's memory. Compare pure decoders with each
other and decode-plus-write tools with each other; the two groups measure
different work.

## Hardware

Results may vary based on:
- CPU (single-threaded performance)
- Disk I/O speed
- Memory bandwidth

The benchmarks above were run on Apple M1.
