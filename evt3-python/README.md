# EVT3 Python Bindings

High-performance EVT 3.0 decoder for Prophesee event cameras with zero-copy numpy support.

## Installation

Supported Python versions: CPython 3.9 through 3.14, including free-threaded
CPython 3.14.

```bash
# From source (requires Rust toolchain)
cd evt3-python
pip install maturin
maturin develop

# Or build a wheel
maturin build --release --interpreter python
pip install target/wheels/evt3-*.whl
```

## Usage

```python
import evt3
import numpy as np

# Decode a raw file
events = evt3.decode_file("recording.raw")

# Access metadata
print(f"Decoded {len(events)} events")
print(f"Sensor: {events.sensor_width}x{events.sensor_height}")

# Access as numpy arrays
x = events.x  # np.ndarray[np.uint16]
y = events.y  # np.ndarray[np.uint16]
p = events.polarity  # np.ndarray[np.uint8] (0=OFF, 1=ON)
t = events.timestamp  # np.ndarray[np.uint64] (microseconds)

# Short aliases also work
p = events.p
t = events.t

# Event arrays are stable objects, so repeated property access is cheap
assert events.x is events.x
assert events.y is events.y
assert events.p is events.p
assert events.t is events.t

# Build an Events container from existing NumPy arrays
events = evt3.Events.from_arrays(
    x=x,
    y=y,
    p=p,
    t=t,
    geometry=(1280, 720),
    copy=False,
)

# Publish decoded or transformed arrays into a running Augur session
evt3.augur.publish_events(events, name="recording-analysis-window")

with evt3.augur.connect() as augur:
    augur.publish_events(events, name="raw")

# Get as dictionary (useful for pandas)
import pandas as pd
df = pd.DataFrame(events.to_dict())

# Decode with trigger events
events, triggers = evt3.decode_file_with_triggers("recording.raw")
trigger_times = triggers.timestamp
trigger_values = triggers.value

# Decode raw bytes (for streaming)
with open("recording.raw", "rb") as f:
    raw_bytes = f.read()
events = evt3.decode_bytes(raw_bytes, sensor_width=1280, sensor_height=720)

# Recommended for large files: bounded-memory file batches
for batch in evt3.decode_file_batches("recording.raw", batch_bytes=8 << 20):
    analyze(batch.x, batch.y, batch.p, batch.t)

# Bounded batches including external trigger events
for events, triggers in evt3.decode_file_batches_with_triggers("recording.raw"):
    analyze(events, triggers.timestamp, triggers.id, triggers.value)

# Recommended for live input: one stateful decoder across byte chunks
decoder = evt3.Decoder(sensor_width=1280, sensor_height=720)
for raw_chunk in camera_chunks:
    analyze(decoder.feed(raw_chunk))
decoder.finish()
```

## Performance

The decoder is implemented in Rust with careful attention to performance:
- Streaming buffer decoding to handle large files
- Columnar data layout for cache-efficient numpy access
- Minimal memory allocations during decoding

The established `decode_file`, `decode_file_with_triggers`, and `decode_bytes`
calls remain compatible. `decode_file` now writes directly into the final
NumPy columns and releases the GIL while Rust reads and decodes the file.
`decode_file_batches` and `decode_file_batches_with_triggers` are opt-in and
keep event storage bounded. During normal iteration, the consumer's current
arrays and the next batch being built can briefly coexist.
