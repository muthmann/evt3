# Python Event Ingress

## Summary

The Python package exposes decoded EVT3 event streams as stable NumPy arrays
and can publish those arrays to an Augur Python ingress listener over the
copy-based protocol v1.

The first connector stage intentionally packs one chunk at a time into Augur's
`packed_xypt_v1` record format. It does not attempt shared memory, Arrow, or
zero-copy cross-process transfer.

## User Impact

- `evt3.decode_file(...)`, `evt3.decode_bytes(...)`, and
  `evt3.decode_file_with_triggers(...)` return `evt3.Events` objects whose
  event-array properties are stable:

  ```python
  assert events.x is events.x
  assert events.y is events.y
  assert events.p is events.p
  assert events.t is events.t
  ```

- `evt3.decode_file_batches(...)` returns bounded CD-event batches, while
  `evt3.decode_file_batches_with_triggers(...)` returns bounded
  `(Events, TriggerEvents)` pairs without dropping external trigger edges.

- `evt3.Events.from_arrays(...)` builds a container from existing NumPy arrays:

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

- `evt3.augur.publish_events(...)` sends either an `Events` object or explicit
  `x`, `y`, `p`, `t` arrays to Augur:

  ```python
  evt3.augur.publish_events(events, name="recording-analysis-window")
  evt3.augur.publish_events(x=x, y=y, p=p, t=t, geometry=(1280, 720))
  ```

- Repeated sends can reuse a TCP session:

  ```python
  with evt3.augur.connect() as augur:
      augur.publish_events(events, name="raw")
      augur.publish_events(filtered, name="filtered")
  ```

## Validation

The connector accepts one-dimensional integer arrays with equal lengths. Floats,
ragged inputs, missing geometry, out-of-bounds coordinates, unsupported time
units, and decreasing timestamps are rejected with actionable errors.

`time_unit="us"` is the only supported timestamp unit. Geometry is required for
raw arrays and must be positive and fit `uint16`. `evt3.Events` supplies its
stored sensor geometry automatically.

## Protocol

The connector talks to `127.0.0.1:57295` by default and refuses non-loopback
hosts. It sends:

- JSON `hello` handshake with protocol `1`
- JSON `start_events` metadata
- JSON `event_batch` headers followed by length-prefixed binary payloads
- JSON `finish_events`

Each packed event record is 14 bytes:

| Offset | Type | Meaning |
|---:|---|---|
| 0 | little-endian `u16` | `x` |
| 2 | little-endian `u16` | `y` |
| 4 | `u8` | polarity, `0` = OFF, nonzero = ON |
| 5 | `u8` | padding, always `0` |
| 6 | little-endian `u64` | timestamp in microseconds |

## Verification

- `cargo test -p evt3-python` verifies the Rust extension compiles.
- Python tests cover stable array identity, `Events.from_arrays(...)`
  validation and normalization, and connector behavior against a fake Augur
  protocol server.
