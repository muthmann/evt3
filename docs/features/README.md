# Feature Index

- [Byte-Stream Decoding](./byte-stream-decoding.md): Incremental decoding of raw EVT3 byte streams via `Evt3Decoder::decode_bytes` and `finish_stream`.
- [HDF5 File Support](./hdf5-file-support.md): Optional `.h5`/`.hdf5` decoding through the existing `Evt3Decoder::decode_file` API.
- [Python NumPy Events](./python-events.md): Stable event arrays, construction from existing arrays, and validation rules.
- [Bounded-Memory Decoding](./bounded-memory-decoding.md): Direct columnar output and bounded raw/HDF5 batch iteration.
- [Parallel Decoding](./parallel-decoding.md): Optional `parallel` feature, `Parallelism` policies, and Python `threads=` / CLI `--threads`.
- [Performance Optimization](./performance-optimization.md): Hot-path design, build profiles, profiling, and measurement guidance.
- [Python Version Support And Wheel Builds](./python-version-support.md): CPython 3.9-3.14 and free-threaded 3.14 support with per-interpreter wheels.
