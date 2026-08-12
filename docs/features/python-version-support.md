# Python Version Support And Wheel Builds

## Summary

The Python package targets CPython 3.9 through 3.14. This includes both the
standard and free-threaded CPython 3.14 interpreters. The release workflow
builds wheels for every supported interpreter on Linux, macOS, and Windows.

## User Impact

- `pip install evt3` has published wheel coverage for CPython 3.9, 3.10, 3.11,
  3.12, 3.13, and 3.14.
- Free-threaded CPython 3.14 uses a separate `cp314t` wheel. Importing `evt3`
  keeps the GIL disabled.
- Python 3.8 is no longer supported; package metadata now requires Python 3.9+.
- Local source builds continue to use `maturin develop` / `maturin build` and
  require Rust 1.83 or newer.
- HDF5 support remains source-only because it depends on the optional native
  HDF5 toolchain and runtime plugin setup.

## Free-Threading Behavior

- Independent decoder instances can run concurrently.
- Decoded event arrays can be shared between threads for read-only access.
- Concurrent writes to shared NumPy arrays require caller-provided
  synchronization.
- A stateful `Decoder`, `FileDecoder`, or `FileDecoderWithTriggers` instance
  must not be advanced concurrently from several threads.

## Implementation Notes

- `evt3-python/Cargo.toml` uses `pyo3 = 0.29` and `numpy = 0.29`.
- The extension declares `gil_used = false`, and long-running file decoding
  uses `Python::detach`.
- `.github/workflows/ci.yml` runs the Python synthetic test suite on Python 3.9
  through 3.14, checks the Rust 1.83 minimum, and runs dedicated concurrency
  checks on Python 3.14t.
- `.github/workflows/release.yml` builds one wheel per supported interpreter and
  operating system, including `cp314t`, with
  `maturin build --interpreter python`.

## Verification

- `cargo check -p evt3-python`
- `cargo clippy --all-targets -p evt3-python -- -D warnings`
- `cargo fmt --all`
- `pytest tests/ -v -k "not TestDecodeFile"` on each supported interpreter
- Dedicated free-threaded import and parallel decoding tests on CPython 3.14t
