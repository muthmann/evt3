# Python Version Support And Wheel Builds

## Summary

The Python package now targets CPython 3.9 through 3.13. The bindings were
updated to PyO3 and `numpy` crate releases that support Python 3.13, and the
release workflow now builds wheels for every supported interpreter on Linux,
macOS, and Windows instead of only producing Python 3.11 wheels.

## User Impact

- `pip install evt3` now has published wheel coverage for CPython 3.9, 3.10,
  3.11, 3.12, and 3.13.
- Python 3.8 is no longer supported; package metadata now requires Python 3.9+.
- Local source builds continue to use `maturin develop` / `maturin build` and
  still require Rust.
- HDF5 support remains source-only because it depends on the optional native
  HDF5 toolchain and runtime plugin setup.

## Implementation Notes

- `evt3-python/Cargo.toml` now uses `pyo3 = 0.23` and `numpy = 0.23`.
- `evt3-python/src/lib.rs` was migrated to the current PyO3 Bound API used by
  those releases.
- `.github/workflows/ci.yml` runs the Python synthetic test suite on Python 3.9
  through 3.13.
- `.github/workflows/release.yml` builds one wheel per supported interpreter and
  operating system with `maturin build --interpreter python`.

## Verification

- `cargo check -p evt3-python`
- `cargo fmt --all`
- Python build and test verification commands are recorded in `tasks/todo.md`
