# EVT3 Python Bindings

High-performance EVT 3.0 decoder for Prophesee event cameras with zero-copy numpy support.

## Installation

```bash
# From source (requires Rust toolchain)
cd evt3-python
pip install maturin
maturin develop

# Or build a wheel
maturin build --release
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
```

## Performance

The decoder is implemented in Rust with careful attention to performance:
- Streaming buffer decoding to handle large files
- Columnar data layout for cache-efficient numpy access
- Minimal memory allocations during decoding
