# EVT3 Decoder Benchmarks

Performance benchmarks comparing the Rust EVT3 decoder against the C++ reference implementation.

## Latest Results

**Test file:** [`laser.raw`](https://kdrive.infomaniak.com/app/share/975517/71d66a09-e3b6-480b-ba94-a1509e8ab2c8) (325 MB, 116M events)

| Decoder | Avg Time | Events/sec | Speedup |
|---------|----------|------------|---------|
| Rust (Python) | 2.35s | 49M/s | **5.6x** |
| Rust CLI | 9.96s | 12M/s | **1.3x** |
| C++ Reference | 13.13s | 9M/s | 1.0x (baseline) |

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
- **Rust CLI / C++ Reference**: Measures decode + CSV file write (I/O bound)
- Events/sec calculated from average time
- Speedup relative to C++ reference decoder

## Hardware

Results may vary based on:
- CPU (single-threaded performance)
- Disk I/O speed
- Memory bandwidth

The benchmarks above were run on Apple M1.
