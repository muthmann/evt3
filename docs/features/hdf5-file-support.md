# HDF5 File Support

## Summary

`evt3-core` can now decode Prophesee HDF5 recordings (`.h5` and `.hdf5`) through the existing `Evt3Decoder::decode_file` entry point when built with the `hdf5` cargo feature. The CLI and Python bindings expose the same feature as a passthrough, so callers do not need a separate API.

## User Impact

- `.raw` decoding remains the default behavior.
- `.h5` and `.hdf5` inputs are auto-detected by file extension.
- Builds without the `hdf5` feature fail fast with a clear `InvalidFormat` error instead of silently attempting raw decoding.

## Build And Runtime Requirements

- Native HDF5 development files must be installed.
- On macOS with Homebrew, export `HDF5_DIR="$(brew --prefix hdf5)"` before building with `--features hdf5`.
- Compressed Prophesee files that use the ECF codec still require the Metavision HDF5 plugin at runtime via `HDF5_PLUGIN_PATH`.

## Verification

- `cargo test -p evt3-core test_hdf5_requires_feature -- --nocapture`
- `HDF5_DIR=/opt/homebrew/opt/hdf5 cargo test -p evt3-core --features hdf5 test_hdf5_ -- --nocapture`
- `cargo check -p evt3-cli --features hdf5`
- `cargo check -p evt3-python --features hdf5`
