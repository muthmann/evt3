# Bounded-Memory Decoding

## Summary

The decoder supports a sink-based core and bounded file batches. Existing Rust
and Python APIs remain available and use the optimized core internally.

## Python Compatibility

Existing applications do not need changes:

```python
events = evt3.decode_file("recording.raw")
```

This call now decodes directly into the final `uint16`, `uint16`, `uint8`, and
`uint64` NumPy columns. It does not first retain an aligned Rust `CdEvent`
array. The Python GIL is released while file I/O and decoding run.

Applications with recordings larger than available RAM can opt into batches:

```python
for events in evt3.decode_file_batches("recording.raw", batch_bytes=8 << 20):
    update_statistics(events)
```

Each yielded object is the same public `evt3.Events` type used by
`decode_file`. Consumers can migrate one processing loop at a time. They do
not need to change event property names or dtypes.

External trigger events are available without materializing the full file:

```python
for events, triggers in evt3.decode_file_batches_with_triggers(
    "recording.raw", batch_bytes=8 << 20
):
    update_statistics(events)
    record_trigger_edges(triggers.timestamp, triggers.id, triggers.value)
```

The original `decode_file_batches` iterator continues to yield only
`evt3.Events`. The trigger-aware iterator yields an `(Events, TriggerEvents)`
tuple for each input batch. Both paths decode the same underlying stream.

## Rust API

- `decode_file` and `decode_buffer` preserve their row-oriented return types.
- `decode_file_columns` returns NumPy-friendly structure-of-arrays storage.
- `decode_file_into`, `decode_buffer_into`, and `decode_bytes_into` accept an
  `EventSink` for custom storage or immediate processing.
- `EventFileReader` reads raw and optional HDF5 inputs in bounded batches.

## Memory Model

One CD event uses:

- 16 bytes in aligned `CdEvent` row storage.
- 13 bytes in the columnar representation.
- A legacy Python conversion previously held both layouts at the same time.

For the 116,300,447-event `laser.raw` fixture, the column payload is 1.51 GB.
The previous simultaneous row and column payloads required at least 3.37 GB,
excluding allocator capacity and runtime overhead. During normal iteration,
the current consumer batch and the next batch being built can briefly coexist.
Memory still remains bounded when the caller does not save earlier batches.
