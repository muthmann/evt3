# Python NumPy Events

## Summary

The Python package exposes decoded EVT3 streams as stable NumPy arrays. The
same `evt3.Events` container accepts arrays produced by independent analysis
code, so filtering and transformation do not require another decoding pass.

## Decoded Arrays

`evt3.decode_file(...)`, `evt3.decode_bytes(...)`, and
`evt3.decode_file_with_triggers(...)` return `evt3.Events` objects with these
columns:

| Property | Alias | Decoded dtype | Meaning |
|---|---|---|---|
| `x` | — | `uint16` | X coordinate in pixels |
| `y` | — | `uint16` | Y coordinate in pixels |
| `polarity` | `p` | `uint8` | `0` = OFF, `1` = ON |
| `timestamp` | `t` | `uint64` | Native event time in microseconds |

Repeated property access returns the same array objects without copying the
columns:

```python
assert events.x is events.x
assert events.y is events.y
assert events.p is events.polarity
assert events.t is events.timestamp
```

The arrays remain valid for the lifetime of their Python references. They are
mutable NumPy arrays; code that shares them must coordinate mutations itself.
The package provides a data model and decoder, without application-specific
publishing or viewer protocols.

## Constructing Events

```python
import evt3
import numpy as np

events = evt3.Events.from_arrays(
    x=np.array([10, 11], dtype=np.uint16),
    y=np.array([20, 20], dtype=np.uint16),
    p=np.array([0, 1], dtype=np.uint8),
    t=np.array([1000, 1001], dtype=np.uint64),
    geometry=(1280, 720),
    copy=False,
)
```

Inputs must be one-dimensional integer arrays of equal length. Geometry is
required, positive, and representable as `uint16`; coordinates must lie within
its bounds. Timestamps must be nonnegative and nondecreasing. The only
supported timestamp unit is `time_unit="us"`.

With `copy=False`, `x` and `y` must already be `uint16`, polarity must be
`uint8` or `bool`, and timestamps must be `uint64` or nonnegative `int64`.
Polarity values must be `0` or `1`; boolean polarity is converted to `uint8`.
With `copy=True`, compatible integer arrays are normalized to the standard
column dtypes and contiguous layout; nonzero polarity inputs become `1`.
Neither mode sorts timestamps or infers missing geometry.

The long names `polarity=` and `timestamp=` can replace `p=` and `t=`. Supplying
both names for the same column is rejected. `to_dict()` exposes the columns
using their long names for tools such as pandas.

## Bounded Processing

`evt3.decode_file_batches(...)` returns bounded CD-event batches.
`evt3.decode_file_batches_with_triggers(...)` returns bounded
`(Events, TriggerEvents)` pairs, preserving external trigger edges. Arrays in a
batch remain valid after the iterator advances; retaining every batch retains
the complete decoded recording.

For live input, keep one `evt3.Decoder` across successive byte chunks and call
`finish()` when input ends. See [Byte-Stream Decoding](./byte-stream-decoding.md)
for timestamp continuity and [Bounded-Memory Decoding](./bounded-memory-decoding.md)
for file iteration.

## Verification

Python tests cover stable array identity, `Events.from_arrays(...)` validation
and normalization, batch iteration, and stateful byte-stream decoding.
