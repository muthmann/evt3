# HDF5 File Support

## Summary

`evt3-core` can now decode Prophesee HDF5 recordings (`.h5` and `.hdf5`) through the existing `Evt3Decoder::decode_file` entry point when built with the `hdf5` cargo feature. The CLI and Python bindings expose the same feature as a passthrough, so callers do not need a separate API.

## User Impact

- `.raw` decoding remains the default behavior.
- `.h5` and `.hdf5` inputs are auto-detected by file extension.
- Builds without the `hdf5` feature fail fast with a clear `InvalidFormat` error instead of silently attempting raw decoding.

## Limitations & Distribution

HDF5 support is **opt-in and requires a native C library** (`libhdf5`). This has
consequences for each distribution channel:

| Channel | HDF5 available? | Reason |
|---------|----------------|--------|
| `pip install evt3` | **No** | `libhdf5` is a system C library that cannot be bundled into a wheel |
| Pre-built CLI binaries (GitHub Releases) | **No** | Binaries are built without `--features hdf5` |
| `cargo install evt3-cli` | **No** (default) | Must add `--features hdf5` and have `libhdf5` installed |
| Build from source | **Yes** | See below |
| `evt3-core` as a crate dependency | **Yes, opt-in** | Add `features = ["hdf5"]`; requires `libhdf5-dev` at build time |

**Using evt3-core as a dependency with HDF5:**

```toml
# Cargo.toml
[dependencies]
evt3-core = { version = "0.2", features = ["hdf5"] }
```

Your users must have `libhdf5-dev` installed (`brew install hdf5` /
`apt install libhdf5-dev`). For ECF-compressed files the runtime ECF plugin
must also be present — see the ECF Compression Plugin section below.

## Build And Runtime Requirements

- Native HDF5 development files must be installed.
- On macOS with Homebrew, export `HDF5_DIR="$(brew --prefix hdf5)"` before building with `--features hdf5`.
- Compressed Prophesee files that use the ECF codec still require the HDF5 ECF plugin at runtime via `HDF5_PLUGIN_PATH`.

## ECF Compression Plugin

Prophesee HDF5 files are commonly compressed with the ECF codec (filter
`0x8ECF`). Without the plugin, HDF5 decoding returns a clear error; the decoder
itself still works for uncompressed HDF5 files.

The plugin is available as a standalone repository:
[prophesee-ai/hdf5_ecf](https://github.com/prophesee-ai/hdf5_ecf)

No prebuilt releases are published there, so the plugin must be built from
source.

### Automated (recommended)

A helper script handles the full clone-build-install flow:

```bash
./scripts/install-ecf-plugin.sh
./scripts/install-ecf-plugin.sh --prefix /custom/path
./scripts/install-ecf-plugin.sh --force
```

The script auto-detects HDF5 from `HDF5_DIR`, Homebrew, `pkg-config`, or
`h5cc`, installs the plugin into `~/.local/share/hdf5/plugin/` by default, and
prints the `export HDF5_PLUGIN_PATH=...` line to add to your shell config.

### Manual

```bash
# Requires: CMake 3.14+, a C++14 compiler, and HDF5 development files
git clone https://github.com/prophesee-ai/hdf5_ecf.git
cmake -S hdf5_ecf -B hdf5_ecf/build -DCMAKE_BUILD_TYPE=Release \
  -DHDF5_ROOT="$(brew --prefix hdf5)"
cmake --build hdf5_ecf/build --parallel

# Point HDF5 at the built plugin directory
export HDF5_PLUGIN_PATH="$PWD/hdf5_ecf/build/lib/hdf5/plugin"
```

Notes:
- The upstream CMake project sets Apple-specific install defaults and exposes
  `HDF5_ECF_PLUGIN_INSTALL_PATH` if you want a different install location.
- If you prefer an installed plugin over using the build tree directly, run
  `cmake --install hdf5_ecf/build` and point `HDF5_PLUGIN_PATH` at the install
  destination.

### Verifying The Plugin Is Found

```bash
HDF5_PLUGIN_PATH=/your/plugin/path \
HDF5_DIR="$(brew --prefix hdf5)" \
cargo test -p evt3-core --features hdf5 -- --show-output
```

## Verification

- `cargo test -p evt3-core test_hdf5_requires_feature -- --nocapture`
- `HDF5_DIR=/opt/homebrew/opt/hdf5 cargo test -p evt3-core --features hdf5 test_hdf5_ -- --nocapture`
- `HDF5_DIR=/opt/homebrew/opt/hdf5 cargo test -p evt3-core --features hdf5 test_hdf5_real_file_ -- --show-output`
- `cargo check -p evt3-cli --features hdf5`
- `cargo check -p evt3-python --features hdf5`
