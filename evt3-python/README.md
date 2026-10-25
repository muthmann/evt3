# evt3 — highly optimized event camera decoder for Python

[![PyPI](https://img.shields.io/pypi/v/evt3)](https://pypi.org/project/evt3/)
[![Python versions](https://img.shields.io/pypi/pyversions/evt3)](https://pypi.org/project/evt3/)
[![CI](https://github.com/muthmann/evt3/actions/workflows/ci.yml/badge.svg)](https://github.com/muthmann/evt3/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/muthmann/evt3/blob/main/LICENSE-MIT)

Read Prophesee and Metavision `.raw` event camera recordings directly into
NumPy arrays. `evt3` decodes the EVT3 (EVT 3.0) encoding used by Prophesee
event-based vision sensors. The decoder is written in Rust, releases the GIL,
and writes straight into NumPy's columnar layout, so there is no Python-level
parsing loop and no per-event object.

Use it for event-based vision, neuromorphic research, and DVS data analysis
when you need `.raw` files as `x`, `y`, `p`, `t` arrays without installing a
full camera SDK.

- **Highly optimized** — about 390 million events per second from a `.raw`
  file into NumPy, 3.7x faster than evt3 0.4 (Apple M1 Pro, 325 MiB
  recording). The companion CLI exports CSV 1.73x faster than Prophesee's
  official C++ decoder sample, with byte-identical output.
- **Explicit threads** — up to four decoder threads by default; `threads=1`
  decodes serially.
- **NumPy-native** — zero-copy `uint16`/`uint8`/`uint64` arrays, stable across
  repeated access.
- **Bounded memory** — batch iterators for recordings larger than RAM.
- **Streaming** — a stateful decoder for live camera byte chunks.
- **No SDK required** — pure wheel, only NumPy at runtime.

## Installation

```bash
pip install evt3
```

Wheels are published for CPython 3.9 through 3.14 on Linux, macOS, and
Windows, including the free-threaded CPython 3.14 build.

> The wheel supports `.raw` files. HDF5 (`.h5`/`.hdf5`) input needs the native
> `libhdf5` library and must be built from source — see the
> [HDF5 documentation](https://github.com/muthmann/evt3/blob/main/docs/features/hdf5-file-support.md).

## Quick start

```python
import evt3

events = evt3.decode_file("recording.raw")

print(f"Decoded {len(events):,} events")
print(f"Sensor: {events.sensor_width}x{events.sensor_height}")

x = events.x          # np.ndarray[uint16]
y = events.y          # np.ndarray[uint16]
p = events.polarity   # np.ndarray[uint8]  (0=OFF, 1=ON)
t = events.timestamp  # np.ndarray[uint64] (microseconds)

# Short aliases also work
p = events.p
t = events.t

duration_s = (t[-1] - t[0]) / 1e6
print(f"Duration: {duration_s:.2f} s, rate: {len(events) / duration_s:,.0f} ev/s")
```

### Decoder threads

All decoding functions and decoder/iterator constructors accept keyword-only
`threads`: `0` (default) selects Auto (up to four total threads), `1` runs
serially, and `n > 1` caps total decoder threads including the caller.

```python
events = evt3.decode_file("recording.raw", threads=1)
decoder = evt3.Decoder(threads=2)
print(evt3.PARALLEL_ENABLED)
```

Use one decoder thread when the application already decodes files concurrently.
Build with `maturin develop --no-default-features` to omit Rayon entirely;
Auto then runs serially and explicit requests greater than one raise an error.
These controls affect EVT3 decoding; HDF5 uses its existing read path.

### Convert to pandas

```python
import pandas as pd

df = pd.DataFrame(events.to_dict())
```

### Large recordings — bounded memory

`decode_file` holds the whole recording. For files larger than RAM, iterate
batches instead:

```python
for batch in evt3.decode_file_batches("recording.raw", batch_bytes=8 << 20):
    analyze(batch.x, batch.y, batch.p, batch.t)

# Including external trigger edges
for events, triggers in evt3.decode_file_batches_with_triggers("recording.raw"):
    analyze(events, triggers.timestamp, triggers.id, triggers.value)
```

### Live input — stateful streaming decoder

The decoder keeps its state across arbitrary chunk borders, so USB packets do
not need to align to EVT3 word or time boundaries:

```python
decoder = evt3.Decoder(sensor_width=1280, sensor_height=720)
for raw_chunk in camera_chunks:
    analyze(decoder.feed(raw_chunk))
decoder.finish()
```

### External triggers

```python
events, triggers = evt3.decode_file_with_triggers("recording.raw")
trigger_times = triggers.timestamp
trigger_values = triggers.value
```

### Decode from bytes

```python
with open("recording.raw", "rb") as f:
    raw_bytes = f.read()

events = evt3.decode_bytes(raw_bytes, sensor_width=1280, sensor_height=720)
```

### Build `Events` from your own arrays

```python
events = evt3.Events.from_arrays(
    x=x, y=y, p=p, t=t,
    geometry=(1280, 720),
    copy=False,
)
```

### Publish to AugurRS

Send decoded or filtered arrays into a running
[AugurRS](https://github.com/muthmann/augur-rs) session for interactive
preview, 3D raw-event inspection, and viewer tools:

```python
evt3.augur.publish_events(events, name="recording-analysis-window")

with evt3.augur.connect() as augur:
    augur.publish_events(events, name="raw")
    augur.publish_events(filtered_events, name="filtered")
```

## Command-line tool

The same decoder ships as a standalone CLI that converts `.raw` to CSV or a
packed binary format:

```bash
cargo install evt3-cli
evt3 recording.raw events.csv
```

Pre-built binaries for Linux, macOS, and Windows are on the
[releases page](https://github.com/muthmann/evt3/releases). The decoder is also
available as a Rust library with `cargo add evt3`.

## Performance notes

The decoder is implemented in Rust with attention to throughput:

- Raw bytes are decoded in one pass, without an intermediate word buffer
- Complete vector groups are expanded together
- Events go straight into NumPy's columnar layout, with counted allocation
- Large files and chunks (64 KiB or more) are split across decoder threads
  after the exact decoder state at each split point is recovered
- The GIL is released while Rust reads and decodes

Measurements and methods:
[benchmarks](https://github.com/muthmann/evt3/blob/main/benchmarks/README.md).

`decode_file`, `decode_file_with_triggers`, and `decode_bytes` stay
source-compatible. `decode_file` writes directly into the final NumPy columns.
`decode_file_batches` and `decode_file_batches_with_triggers` are opt-in and
keep event storage bounded; during iteration the consumer's current arrays and
the next batch being built can briefly coexist.

## Links

- [Source and full documentation](https://github.com/muthmann/evt3)
- [Issue tracker](https://github.com/muthmann/evt3/issues)
- [Changelog](https://github.com/muthmann/evt3/blob/main/CHANGELOG.md)
- [Prophesee EVT 3.0 format specification](https://docs.prophesee.ai/stable/data/encoding_formats/evt3.html)

## License

MIT — see [LICENSE-MIT](https://github.com/muthmann/evt3/blob/main/LICENSE-MIT).
