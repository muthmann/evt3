# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `evt3.__version__` reported a stale hardcoded value — the published 0.4.0
  wheel identified itself as `0.3.0`. It is now read from the installed
  distribution metadata, so it cannot drift from the release again, and a test
  asserts the two agree.
- The CLI identified itself as `evt3-decode` in `--version` and `--help`, while
  the installed binary is `evt3`
- `--help` showed the crates.io listing description, which is written for the
  registry and reads oddly for someone already running the tool

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
