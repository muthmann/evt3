# EVT3 Decoder

[![CI](https://github.com/muthmann/evt3/actions/workflows/ci.yml/badge.svg)](https://github.com/muthmann/evt3/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/evt3)](https://pypi.org/project/evt3/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)

High-performance EVT 3.0 raw data decoder for [Prophesee](https://www.prophesee.ai/) event cameras, written in Rust.

**5.6x faster than the C++ reference implementation** with byte-for-byte identical output.

## Features

- 🚀 **High Performance** - 50M+ events/second, 5.6x faster than C++ reference
- 📦 **Multiple Interfaces** - CLI tool, Python bindings, Rust library
- 🐍 **Zero-copy Python** - NumPy array access via PyO3
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

### Build from Source

```bash
# Clone repository
git clone https://github.com/muthmann/evt3.git
cd evt3

# Build CLI (requires Rust)
cargo build --release

# The binary is at: ./target/release/evt3
./target/release/evt3 recording.raw events.csv

# Optional: Install to PATH
cp target/release/evt3 ~/.local/bin/

# Build Python package (requires uv + Rust)
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

# Decode a file
events = evt3.decode_file("recording.raw")
print(f"Decoded {len(events):,} events")
print(f"Sensor: {events.sensor_width}x{events.sensor_height}")

# Access as NumPy arrays (zero-copy)
x = events.x          # np.ndarray[uint16]
y = events.y          # np.ndarray[uint16]
p = events.polarity   # np.ndarray[uint8]
t = events.timestamp  # np.ndarray[uint64] (microseconds)

# Basic analysis
print(f"Duration: {(t[-1] - t[0]) / 1e6:.2f} seconds")
print(f"Event rate: {len(events) / ((t[-1] - t[0]) / 1e6):.0f} events/sec")

# Create pandas DataFrame
import pandas as pd
df = pd.DataFrame(events.to_dict())
```

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
