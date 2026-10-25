# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-10-25

### Added

- Optional parallel decoding. The `parallel` Cargo feature adds
  `Parallelism::{Off, Auto, Threads(n)}`, set with
  `Evt3Decoder::with_parallelism` / `set_parallelism` and
  `EventFileReader::with_parallelism`. Only large even byte chunks (64 KiB or
  more) use workers. Each decoder owns its pool; there is no global pool.
- New concrete column APIs `decode_bytes_columns` and
  `EventFileReader::read_next_columns` that follow the configured policy.
- Python: keyword-only `threads=` on every decoding function and
  decoder/iterator constructor, and `evt3.PARALLEL_ENABLED`.
- CLI: `--threads` (0 = Auto, 1 = serial, n = total thread cap).
- A unified benchmark suite that compares the official Prophesee C++ sample,
  a selected pre-change Rust commit and the current decoder, with packet
  latency samples, input/binary hashes and machine-readable results.

### Changed

- Faster serial decoding: byte conversion is fused with decoding, initial
  timestamp synchronization moved out of the main loop, complete 12/12/8-bit
  vector groups are expanded together, and built-in outputs are written in
  bounded batches with counted reservations.
- Defaults: the Rust library decodes serially (`Parallelism::Off`); Python and
  the CLI select Auto. Generic `EventSink` APIs (`decode_bytes_into`,
  `decode_file_into`, `decode_buffer_into`) always decode serially.
- Rayon is an optional dependency. Builds without `parallel` keep all serial
  speedups.
- Whole-file decoding of a 325 MiB recording (116 M events) into NumPy takes
  0.30 s instead of 1.11 s (3.7x, about 390 M events/s with Auto on 8 logical
  CPUs). Without workers, decoding is about 1.4–1.8x faster than 0.4.
- CSV export is 1.73x faster than Prophesee's official standalone C++ sample,
  with byte-identical output. See [benchmarks](benchmarks/README.md).

### Fixed

- Removed cross-workflow speedup ratios from the legacy benchmark summary;
  Python decode-to-memory and CLI CSV export perform different work.

## [0.4.1] - 2026-10-24

### Fixed

- `evt3.__version__` reported a stale hardcoded value — the published 0.4.0
  wheel identified itself as `0.3.0`. It is now read from the installed
  distribution metadata, so it cannot drift from the release again, and a test
  asserts the two agree.
- The CLI identified itself as `evt3-decode` in `--version` and `--help`, while
  the installed binary is `evt3`
- `--help` showed the crates.io listing description, which is written for the
  registry and reads oddly for someone already running the tool
- The workspace `authors` entry in `Cargo.toml` was missing the closing `>`
  of the e-mail address

### Deprecated

- The old crate name `evt3-core` gets a final release, 0.4.1, that re-exports
  `evt3` and says so on crates.io. Depend on `evt3` instead. See
  [ADR 0006](docs/adr/0006-deprecate-evt3-core-crate.md).

## [0.4.0] - 2026-08-12

### Added

- Python 3.14 and free-threaded Python 3.14 support, including CI coverage and
  release wheels for both interpreter variants
- Automated crates.io publishing in the release workflow, gated on the
  `CARGO_REGISTRY_TOKEN` repository secret

### Changed

- Updated the Python bindings to PyO3 0.29 and `numpy` 0.29
- Raised the Rust requirement for the Python bindings to Rust 1.83
- **Breaking (Rust library):** the decoder library is now published as `evt3`
  instead of `evt3-core`. Replace `evt3-core = "0.2"` with `evt3 = "0.4"` and
  `use evt3_core::…` with `use evt3::…`. The source directory is unchanged.
  See [ADR 0005](docs/adr/0005-crate-naming-on-crates-io.md).
- The CLI is now published as `evt3-cli` and installs with
  `cargo install evt3-cli`. The binary is still named `evt3`, so command-line
  usage does not change.
- The internal library dependency is declared once in
  `[workspace.dependencies]` and carries an explicit version, which crates.io
  requires for publishing

### Fixed

- `evt3-cli` declared six crates.io keywords, one more than the registry
  permits, which blocked publishing

## [0.2.0] - 2026-03-08

### Added

- `evt3-core`: optional HDF5 input support via the `hdf5` cargo feature —
  `.h5` and `.hdf5` files are auto-detected by `Evt3Decoder::decode_file`
  with no API changes required for callers
- `evt3-core`: incremental raw-byte decoding via `Evt3Decoder::decode_bytes`
  and `Evt3Decoder::finish_stream`, including odd-chunk boundary handling for
  live camera pipelines
- `evt3-cli`: `hdf5` feature flag propagates HDF5 support to the CLI binary
- `evt3-python`: `hdf5` feature flag propagates HDF5 support to Python bindings

### Notes

- HDF5 support requires native HDF5 development libraries (`brew install hdf5`
  on macOS, `apt install libhdf5-dev` on Debian/Ubuntu)
- Prophesee recordings compressed with the ECF codec additionally require the
  Metavision HDF5 plugin at `HDF5_PLUGIN_PATH`

## [0.1.0] - 2024-12-28

### Added

- Initial release
- Full EVT 3.0 format support including vectorized events (VECT_12, VECT_8)
- CLI tool (`evt3-decode`) with CSV and binary output formats
- Python bindings with zero-copy NumPy array access
- Customizable field order for CSV output
- External trigger event support
- File header parsing for sensor metadata
- TIME_HIGH loop detection for recordings >16.7 seconds
- Comprehensive test suite validated against C++ reference implementation

### Performance

- 1.6x faster than C++ reference implementation
- 9.2M events/second throughput on Apple M1
