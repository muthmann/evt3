# 0001: HDF5 File Support Through `decode_file`

## Status

Accepted

## Context

The project already exposes file decoding through `Evt3Decoder::decode_file`, and both the CLI and Python bindings delegate to that method. Supporting Prophesee HDF5 recordings should not require a second public entry point or caller-side format switches.

The original plan proposed the `hdf5` crate version `0.8`. In this environment that crate does not build against Homebrew HDF5 `1.14.5`, so the implementation uses the maintained `hdf5-metno` package aliased locally as `hdf5` while keeping the cargo feature name `hdf5`.

## Decision

- Keep `Evt3Decoder::decode_file` as the single public file-decoding API.
- Auto-detect HDF5 input by `.h5` and `.hdf5` file extension.
- Gate HDF5 support behind the optional `hdf5` cargo feature.
- Implement HDF5 parsing in an internal `hdf5_decoder` module.
- Preserve existing `.raw` behavior and return an explicit error when HDF5 input is used without the feature.

## Consequences

- CLI and Python callers gain HDF5 support without source changes.
- The core crate now depends on native HDF5 libraries only when the feature is enabled.
- Build instructions must document `HDF5_DIR` and runtime plugin requirements for compressed Prophesee files.
