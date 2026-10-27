# evt3 — highly optimized EVT3 decoder for Prophesee event cameras

[![CI](https://github.com/muthmann/evt3/actions/workflows/ci.yml/badge.svg)](https://github.com/muthmann/evt3/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/evt3)](https://pypi.org/project/evt3/)
[![crates.io](https://img.shields.io/crates/v/evt3)](https://crates.io/crates/evt3)
[![docs.rs](https://img.shields.io/docsrs/evt3)](https://docs.rs/evt3)
[![Python versions](https://img.shields.io/pypi/pyversions/evt3)](https://pypi.org/project/evt3/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)

Read [Prophesee](https://www.prophesee.ai/) and Metavision event camera
recordings into NumPy, CSV, or Rust. `evt3` decodes the EVT3 (EVT 3.0)
encoding used by Prophesee event-based vision sensors, from `.raw` and
optionally HDF5 files. It ships as a command-line tool, a Python package, and
a Rust library.

**About 390 million events per second** from a `.raw` recording into NumPy,
3.7x faster than evt3 0.4. CSV export is **1.73x faster than Prophesee's
official C++ decoder sample**, with byte-identical output.
[Measured](#performance) on an Apple M1 Pro with a 325 MiB recording.

Built for event-based vision, neuromorphic engineering, and DVS data analysis
when you need event streams as plain `x`, `y`, `p`, `t` arrays without
installing a full camera SDK.

```bash
pip install evt3         # Python + NumPy
cargo install evt3-cli   # command-line tool
cargo add evt3           # Rust library
```

## Features

- 🚀 **Highly Optimized** - fused byte decoding, grouped vector expansion, bounded output writers, and optional parallel decoding of large chunks
- 🧵 **Explicit Parallelism** - serial by default in Rust; Python and CLI use up to four threads and accept `threads=1` / `--threads 1`
- 📦 **Multiple Interfaces** - CLI tool, Python bindings, Rust library
- 🐍 **NumPy-native Python** - stable `Events` arrays for analysis without clone-on-access surprises
- 🧪 **Optional HDF5 Input** - `.h5` and `.hdf5` support behind a cargo feature
- ✅ **Validated** - Checked CSV output matches Prophesee's C++ sample byte for byte, serial and parallel
- 🔧 **Customizable** - Configurable output field order

## Quick Start

### Install with Cargo

```bash
cargo install evt3-cli
```

The `evt3-cli` crate installs a binary named `evt3`. To use the decoder as a
Rust library instead, depend on the `evt3` crate:

```bash
cargo add evt3
```

### One-Line Install (Linux/macOS)

```bash
curl -sSL https://raw.githubusercontent.com/muthmann/evt3/main/install.sh | bash
```

This downloads the binary to `~/.local/bin/evt3`. You may need to add it to your PATH:

```bash
# Add to ~/.bashrc or ~/.zshrc
export PATH="$HOME/.local/bin:$PATH"
```

### Download Pre-built Binary

Download from [Releases](https://github.com/muthmann/evt3/releases):

| Platform | Binary |
|----------|--------|
| Linux x64 | `evt3-linux-x64` |
| Linux ARM64 | `evt3-linux-arm64` |
| macOS Intel | `evt3-macos-x64` |
| macOS Apple Silicon | `evt3-macos-arm64` |
| Windows | `evt3-windows-x64.exe` |

```bash
# Example for macOS Apple Silicon
curl -LO https://github.com/muthmann/evt3/releases/latest/download/evt3-macos-arm64
chmod +x evt3-macos-arm64
./evt3-macos-arm64 recording.raw events.csv
```

### Python Package

Using [uv](https://docs.astral.sh/uv/) (recommended):

```bash
uv pip install evt3
```

Or with pip:

```bash
pip install evt3
```

Published wheels target CPython 3.9 through 3.14, including the free-threaded
CPython 3.14 build.

> **Note:** The pip package supports `.raw` files only. HDF5 (`.h5`/`.hdf5`)
> requires building from source — see [HDF5 Inputs](#hdf5-inputs) below.

### Build from Source

```bash
# Clone repository
git clone https://github.com/muthmann/evt3.git
cd evt3

# Build CLI (requires Rust)
cargo build --release

# The binary is at: ./target/release/evt3
./target/release/evt3 recording.raw events.csv

# Optional HDF5 support
HDF5_DIR="$(brew --prefix hdf5)" cargo build --release -p evt3-cli --features hdf5

# Optional: Install to PATH
cp target/release/evt3 ~/.local/bin/

# Build Python package (requires Python 3.9+, uv + Rust 1.83+)
cd evt3-python
uv venv
uv pip install maturin
source .venv/bin/activate
maturin develop --release
```

## Usage

### CLI

```bash
# Decode to CSV (default: x,y,p,t)
evt3 recording.raw events.csv

# Timestamp-first format
evt3 recording.raw events.csv --format "t,x,y,p"

# Binary output (more efficient)
evt3 recording.raw events.bin

# Include trigger events
evt3 recording.raw events.csv --triggers triggers.csv

# Quiet mode
evt3 recording.raw events.csv --quiet

# Decoder threads: 0 = Auto (default, up to 4), 1 = serial, n = cap
evt3 recording.raw events.csv --threads 1
```

### Python

```python
import evt3
import numpy as np

# Decode a .raw or .h5 file — format is auto-detected by extension
events = evt3.decode_file("recording.raw")
events = evt3.decode_file("recording.h5")   # requires hdf5 feature at build time
print(f"Decoded {len(events):,} events")
print(f"Sensor: {events.sensor_width}x{events.sensor_height}")

# Access as NumPy arrays (zero-copy)
x = events.x          # np.ndarray[uint16]
y = events.y          # np.ndarray[uint16]
p = events.polarity   # np.ndarray[uint8]
t = events.timestamp  # np.ndarray[uint64] (microseconds)

# Repeated access returns stable array objects
assert events.x is events.x
assert events.y is events.y
assert events.p is events.p
assert events.t is events.t

# Basic analysis
print(f"Duration: {(t[-1] - t[0]) / 1e6:.2f} seconds")
print(f"Event rate: {len(events) / ((t[-1] - t[0]) / 1e6):.0f} events/sec")

# Create pandas DataFrame
import pandas as pd
df = pd.DataFrame(events.to_dict())

# Decoder threads: 0 = Auto (default, up to 4), 1 = serial, n = cap.
# Use threads=1 when your application already decodes files in parallel.
events = evt3.decode_file("recording.raw", threads=1)

# Existing decode_file code stays unchanged and now uses the optimized
# columnar decoder internally. Process bounded batches when the full recording
# does not need to stay in memory:
for batch in evt3.decode_file_batches("recording.raw", batch_bytes=8 << 20):
    process(batch.x, batch.y, batch.p, batch.t)

# Preserve external trigger events in the bounded-memory workflow:
for events, triggers in evt3.decode_file_batches_with_triggers("recording.raw"):
    process(events, triggers.timestamp, triggers.id, triggers.value)

# Preserve decoder state across arbitrary live-input chunk borders:
decoder = evt3.Decoder(sensor_width=1280, sensor_height=720)
for raw_chunk in camera_chunks:
    process(decoder.feed(raw_chunk))
decoder.finish()
```

### Construct Python Events From Arrays

Use the same `Events` model for decoded data and arrays produced by your own
analysis:

```python
events = evt3.Events.from_arrays(
    x=x,
    y=y,
    p=p,
    t=t,
    geometry=(1280, 720),
    copy=False,
)
```

The constructor validates shape, dtype, coordinate bounds, polarity, and
nondecreasing microsecond timestamps. See [Python NumPy Events](docs/features/python-events.md)
for the array contract and normalization options.

### Rust Library

```bash
cargo add evt3
```

```rust
use evt3::Evt3Decoder;

let mut decoder = Evt3Decoder::new();
let result = decoder.decode_file("recording.raw")?;

println!("Decoded {} events", result.cd_events.len());
for event in result.cd_events.iter().take(10) {
    println!("x={}, y={}, p={}, t={}", 
        event.x, event.y, event.polarity, event.timestamp);
}
```

For live camera pipelines or embedded integrations, you can stream raw USB
packet bytes directly into the decoder without converting to `Vec<u16>` first:

```rust
use evt3::Evt3Decoder;

let mut decoder = Evt3Decoder::new();
let mut cd_events = Vec::new();
let mut trigger_events = Vec::new();

for chunk in usb_packet_chunks {
    decoder.decode_bytes(chunk, &mut cd_events, &mut trigger_events)?;
}

decoder.finish_stream()?;
```

This keeps `evt3` usable in incremental preview paths while preserving the
existing file and word-based APIs.

The library decodes serially by default. Enable the `parallel` feature and
choose a policy to decode large files and chunks on several threads:

```toml
evt3 = { version = "0.5", features = ["parallel"] }
```

```rust
use evt3::{Evt3Decoder, Parallelism};

let mut decoder = Evt3Decoder::new().with_parallelism(Parallelism::Auto);
let result = decoder.decode_file("recording.raw")?;
```

Only even byte chunks of at least 64 KiB use workers. Generic `EventSink` APIs
such as `decode_bytes_into` always run serially. See
[Parallel Decoding](docs/features/parallel-decoding.md) for all policies.

Notes:
- `decode_buffer` still expects 16-bit EVT3 words, not raw bytes.
- `decode_bytes` expects little-endian EVT3 payload bytes and can be called with odd-sized chunks.
- Call `finish_stream()` only when the stream is complete so a trailing half-word is reported as an error instead of being buffered for the next chunk.
- `.h5` and `.hdf5` decoding is available when the crate or binary is built with the `hdf5` feature.
- `decode_file` remains source-compatible and returns the same `Events` API. It
  now decodes directly into NumPy's columnar layout and releases the Python GIL.
- `decode_file_batches` is the bounded-memory option for large recordings.
  Use `decode_file_batches_with_triggers` when external trigger edges are also
  required. The arrays in a batch remain valid after the iterator advances,
  but retaining all batches naturally retains the full recording.

### HDF5 Inputs

> **Important:** HDF5 support requires `libhdf5` (a native C library) and is
> **not included** in `pip install evt3` or pre-built CLI binaries. It must be
> built from source. See [docs/features/hdf5-file-support.md](docs/features/hdf5-file-support.md)
> for the full limitations table.

```bash
# macOS
brew install hdf5
HDF5_DIR="$(brew --prefix hdf5)" cargo build --release -p evt3-cli --features hdf5
./target/release/evt3 recording.h5 events.csv

# Ubuntu / Debian
sudo apt install libhdf5-dev
cargo build --release -p evt3-cli --features hdf5
./target/release/evt3 recording.h5 events.csv
```

Most Prophesee HDF5 files use the ECF compression codec, which requires an
additional runtime plugin:

```bash
# Build and install the ECF plugin (one-time, macOS/Linux/Windows)
./scripts/install-ecf-plugin.sh

# Then set the plugin path before running
export HDF5_PLUGIN_PATH="$HOME/.local/share/hdf5/plugin"
./target/release/evt3 recording.h5 events.csv
```

Notes:
- Builds without `--features hdf5` return a clear error rather than silently failing.
- Real-data integration tests that skip still show as `ok`. Run with `-- --show-output` to see `[SKIP]` reasons.
- Full plugin and dependency documentation: [docs/features/hdf5-file-support.md](docs/features/hdf5-file-support.md)

## Performance

Apple M1 Pro (8 cores), macOS, Rust 1.95. Input: `laser.raw`
(325 MiB, 116,300,447 CD events), warm file cache, median of five runs.
Baseline: evt3 0.4.

| Workflow | evt3 0.4 | Current | Speedup | Events/s |
|---|---:|---:|---:|---:|
| Python `decode_file` (Auto) | 1.110 s | 0.301 s | 3.7x | 387 M |
| Rust `decode_file`, rows (Auto) | 0.896 s | 0.293 s | 3.1x | 396 M |
| Rust `decode_file_columns` (Auto) | 0.943 s | 0.277 s | 3.4x | 421 M |

Auto uses up to four threads, so these gains include extra CPU cores. With
one thread, decoding is about 1.4–1.8x faster than evt3 0.4, measured on
recorded packet replay with 4–64 KiB chunks.

Same work for all decoders: decode the complete file, format CSV, write it to
`/dev/null`. The C++ program is Prophesee's
[official standalone sample](https://github.com/prophesee-ai/openeb/blob/main/standalone_samples/metavision_evt3_raw_file_decoder/metavision_evt3_raw_file_decoder.cpp),
built with `-O3 -DNDEBUG`. It is not the full Metavision SDK.

| CSV export | Time | Events/s | vs. C++ |
|---|---:|---:|---:|
| **evt3 CLI (current)** | **6.66 s** | **17.5 M** | **1.73x** |
| evt3 CLI 0.4 | 7.37 s | 15.8 M | 1.57x |
| Prophesee C++ sample | 11.55 s | 10.1 M | 1.00x |

The whole-file results come from the
[25 October report](benchmarks/final-results-2026-10-25.md); the one-thread
results come from the [parallelism checks](benchmarks/parallelism-controls-2026-10-25.md).
Methods, raw samples, and all workloads: [benchmarks/README.md](benchmarks/README.md).

```bash
cargo bench -p evt3
evt3-python/.venv/bin/python benchmarks/benchmark.py --comparison-suite \
  --rust-baseline-ref 8554807 --iterations 5
```

## Output Formats

### CSV

Human-readable, with optional geometry header:
```csv
%geometry:1280,720
642,481,1,10960097
783,415,1,10960139
...
```

### Binary (.bin)

Efficient packed format for programmatic access:
- 8-byte magic header: `EVT3BIN\0`
- 24-byte metadata: version, width, height, event count
- Events: 14 bytes each (x:u16, y:u16, polarity:u8, pad:u8, timestamp:u64)

## EVT 3.0 Format

EVT 3.0 is a 16-bit vectorized event encoding from Prophesee. This decoder supports:

| Event Type | Code | Description |
|------------|------|-------------|
| EVT_ADDR_Y | 0x0 | Y coordinate |
| EVT_ADDR_X | 0x2 | Single event (X + polarity) |
| VECT_BASE_X | 0x3 | Base X for vectors |
| VECT_12 | 0x4 | 12-event vector |
| VECT_8 | 0x5 | 8-event vector |
| EVT_TIME_LOW | 0x6 | Lower 12 bits of timestamp |
| EVT_TIME_HIGH | 0x8 | Upper 12 bits of timestamp |
| EXT_TRIGGER | 0xA | External trigger |

For full specification: [Prophesee EVT 3.0 Documentation](https://docs.prophesee.ai/stable/data/encoding_formats/evt3.html)

## Project Structure

```
evt3/
├── evt3-core/       # Rust decoder library      -> crate `evt3`
├── evt3-cli/        # Command-line tool         -> crate `evt3-cli`, binary `evt3`
├── evt3-python/     # Python bindings (PyO3)    -> PyPI package `evt3`
├── benchmarks/      # Benchmark suite and measured results
├── docs/            # Feature documentation and ADRs
└── test_data/       # Local recordings for benchmarks (not in git)
```

## Related Projects

- [AugurRS](https://github.com/muthmann/augur-rs) — an event camera workbench
  to record, replay, and analyze event streams, with linked 2D and 3D views
  and a plugin API.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

Licensed under the MIT License - see [LICENSE-MIT](LICENSE-MIT) for details.

## Acknowledgments

- [Prophesee](https://www.prophesee.ai/) for the EVT 3.0 format specification
- The [OpenEB](https://github.com/prophesee-ai/openeb) project for the reference implementation
