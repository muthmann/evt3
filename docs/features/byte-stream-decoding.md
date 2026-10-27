# Incremental Byte-Stream Decoding

## Summary

`evt3` supports incremental decoding of raw EVT3 byte streams through
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

## Timestamp Contract

CD events and external trigger events carry native sensor timestamps in
microseconds. One decoder maintains a shared timestamp epoch for both event
types and extends the sensor's 24-bit clock across observed wraps
(`2^24` microseconds, about 16.78 seconds). Output timestamps already include
those observed wrap epochs; consumers must not apply another modulo-clock
unwrap to them.

Keep the same decoder state across consecutive chunks of one stream. Resetting
or constructing a decoder starts a new history. Starting at a byte offset in
the middle of a recording can synchronize to later time words, but cannot
infer how many clock wraps occurred before that offset or recover other state
that was never supplied. Decode from the beginning when the full recording
epoch is required.

Consumers own any alignment to wall time, another device clock, or an external
recording epoch. Apply that mapping consistently to CD and trigger timestamps.
The decoder does not infer missing history or an external clock offset.

## Verification

- `cargo test -p evt3` — unit tests cover byte-by-byte, odd-chunk, and
  multi-split scenarios as well as finish_stream error and leniency paths.
