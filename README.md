# EVT3 Decoder

[![CI](https://github.com/muthmann/evt3/actions/workflows/ci.yml/badge.svg)](https://github.com/muthmann/evt3/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/evt3)](https://pypi.org/project/evt3/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)

High-performance EVT 3.0 raw data decoder for [Prophesee](https://www.prophesee.ai/) event cameras, written in Rust.

**5.6x faster than the C++ reference implementation** with byte-for-byte identical output.

## Features

- 🚀 **High Performance** - 50M+ events/second, 5.6x faster than C++ reference
- 📦 **Multiple Interfaces** - CLI tool, Python bindings, Rust library
- 🐍 **NumPy-native Python** - stable `Events` arrays for analysis without clone-on-access surprises
- 🔭 **AugurRS Ingress** - publish decoded or transformed NumPy event arrays into [AugurRS](https://github.com/muthmann/augur-rs) for interactive preview, 3D inspection, viewer tools, and plugin workflows
- 🧪 **Optional HDF5 Input** - `.h5` and `.hdf5` support behind a cargo feature
- ✅ **Validated** - Output matches C++ reference implementation exactly
- 🔧 **Customizable** - Configurable output field order

## Quick Start

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

Published wheels target CPython 3.9 through 3.13.

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

# Build Python package (requires Python 3.9+, uv + Rust)
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

### Python To AugurRS

`evt3` can publish decoded or transformed NumPy event arrays into a running
[AugurRS](https://github.com/muthmann/augur-rs) session. This makes Python a
lightweight analysis and filtering environment while AugurRS provides the
interactive event-camera application: live-style preview, 3D raw-event
inspection, viewer tools, exports, and plugins.

```python
import evt3

events = evt3.decode_file("recording.raw")

# Optional Python-side filtering or analysis.
x = events.x
y = events.y
p = events.p
t = events.t

evt3.augur.publish_events(
    x=x,
    y=y,
    p=p,
    t=t,
    geometry=events.sensor_size,
    name="recording-analysis-window",
)
```

You can also create an `Events` container from existing NumPy arrays:

```python
events = evt3.Events.from_arrays(
    x=x,
    y=y,
    p=p,
    t=t,
    geometry=(1280, 720),
    copy=False,
)

evt3.augur.publish_events(events, name="filtered-events")
```

For repeated sends, reuse the loopback session:

```python
with evt3.augur.connect() as augur:
    augur.publish_events(events, name="raw")
    augur.publish_events(filtered_events, name="filtered")
```

The first ingress stage is deliberately copy-based and bounded: event chunks
are packed into AugurRS' 14-byte `packed_xypt_v1` decoded-event transport and
sent over loopback TCP. The connector validates dtype, shape, geometry, and
timestamp ordering before sending so mistakes fail close to the Python code.

### Rust Library

```rust
use evt3_core::Evt3Decoder;

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
use evt3_core::Evt3Decoder;

let mut decoder = Evt3Decoder::new();
let mut cd_events = Vec::new();
let mut trigger_events = Vec::new();

for chunk in usb_packet_chunks {
    decoder.decode_bytes(chunk, &mut cd_events, &mut trigger_events)?;
}

decoder.finish_stream()?;
```

This keeps `evt3-core` usable in incremental preview paths while preserving the
existing file and word-based APIs.

Notes:
- `decode_buffer` still expects 16-bit EVT3 words, not raw bytes.
- `decode_bytes` expects little-endian EVT3 payload bytes and can be called with odd-sized chunks.
- Call `finish_stream()` only when the stream is complete so a trailing half-word is reported as an error instead of being buffered for the next chunk.
- `.h5` and `.hdf5` decoding is available when the crate or binary is built with the `hdf5` feature.
- `decode_file` remains source-compatible and returns the same `Events` API. It
  now decodes directly into NumPy's columnar layout and releases the Python GIL.
- `decode_file_batches` is the bounded-memory option for large recordings. The
  arrays in a batch remain valid after the iterator advances, but retaining all
  batches naturally retains the full recording.

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

## Benchmarks

Tested on Apple M1 with `laser.raw` (325MB, 116M events):

| Decoder | Events/sec | Speedup |
|---------|------------|---------|
| **Rust (Python)** | 49M/s | **5.6x** |
| Rust CLI | 12M/s | 1.3x |
| C++ Reference | 9M/s | 1.0x |

> **Note:** Python benchmark measures pure decode speed. CLI benchmarks include CSV file I/O overhead.

Run benchmarks yourself:
```bash
cargo bench
python benchmarks/benchmark.py
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
├── evt3-core/       # Core Rust decoder library
├── evt3-cli/        # Command-line tool
├── evt3-python/     # Python bindings (PyO3)
├── benchmarks/      # Performance benchmarks
└── test_data/       # Sample EVT3 files
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

Licensed under the MIT License - see [LICENSE-MIT](LICENSE-MIT) for details.

## Acknowledgments

- [Prophesee](https://www.prophesee.ai/) for the EVT 3.0 format specification
- The [OpenEB](https://github.com/prophesee-ai/openeb) project for the reference implementation
