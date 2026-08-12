# Incremental Byte-Stream Decoding

## Summary

`evt3-core` supports incremental decoding of raw EVT3 byte streams through
`Evt3Decoder::decode_bytes` and `Evt3Decoder::finish_stream`. Chunks of any
size — including odd-length chunks that split 16-bit words across boundaries —
are handled correctly by buffering the dangling byte until the next call.

## User Impact

- Live camera pipelines can feed arbitrary byte chunks directly without
  buffering the entire recording in memory.
- `decode_bytes` accumulates events into caller-provided `Vec`s across calls,
  allowing incremental processing.
- `finish_stream` must be called when the stream is complete; it returns an
  error if a half-word is still buffered, guarding against truncated input.
- Byte chunks are decoded directly. The decoder no longer materializes an
  intermediate `Vec<u16>` for every chunk.
- Python exposes the same stateful workflow through `evt3.Decoder.feed()` and
  `evt3.Decoder.finish()`.
- `finish_stream_lenient` is available for callers that know a trailing byte is
  benign (e.g. legacy `.raw` files ending with a newline).

## API

```rust
let mut decoder = Evt3Decoder::new();
let mut cd_events = Vec::new();
let mut trigger_events = Vec::new();

for chunk in byte_stream {
    decoder.decode_bytes(&chunk, &mut cd_events, &mut trigger_events)?;
}

decoder.finish_stream()?;
```

```python
decoder = evt3.Decoder(sensor_width=1280, sensor_height=720)
for raw_chunk in camera_chunks:
    events = decoder.feed(raw_chunk)
    process(events)
decoder.finish()
```

## Verification

- `cargo test -p evt3-core` — unit tests cover byte-by-byte, odd-chunk, and
  multi-split scenarios as well as finish_stream error and leniency paths.
