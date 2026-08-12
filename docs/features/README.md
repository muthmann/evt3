# Feature Index

- [Byte-Stream Decoding](./byte-stream-decoding.md): Incremental decoding of raw EVT3 byte streams via `Evt3Decoder::decode_bytes` and `finish_stream`.
- [HDF5 File Support](./hdf5-file-support.md): Optional `.h5`/`.hdf5` decoding through the existing `Evt3Decoder::decode_file` API.
- [Python Event Ingress](./python-event-ingress.md): NumPy-native Python event containers plus copy-based Augur protocol publishing.
- [Bounded-Memory Decoding](./bounded-memory-decoding.md): Direct columnar output and bounded raw/HDF5 batch iteration.
- [Performance Optimization](./performance-optimization.md): Hot-path, build-profile, profiling, and safe parallel-work guidance.
- [Python Version Support And Wheel Builds](./python-version-support.md): CPython 3.9-3.14 and free-threaded 3.14 support with per-interpreter wheels.
