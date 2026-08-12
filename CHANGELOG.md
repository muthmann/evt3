# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Python 3.14 and free-threaded Python 3.14 support, including CI coverage and
  release wheels for both interpreter variants

### Changed

- Updated the Python bindings to PyO3 0.29 and `numpy` 0.29
- Raised the Rust requirement for the Python bindings to Rust 1.83

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
