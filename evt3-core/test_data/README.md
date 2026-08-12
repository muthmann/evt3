# Test Data

Integration tests look for the following files in this directory. Tests skip
gracefully when a file is absent, so they are optional but required for full
coverage.

| File | Size | Description |
|------|------|-------------|
| `laser.raw` | ~325 MB | EVT3 recording of a laser pattern |
| `laser.hdf5` | ~236 MB | Same recording in Prophesee HDF5 format |

Download both files from:
**https://kdrive.infomaniak.com/app/share/975517/ad8aa115-068e-4f29-9d16-663a7a9b5e02**

Place them directly in this directory, then run:

```bash
# EVT3 tests
cargo test -p evt3

# HDF5 tests (requires libhdf5)
HDF5_DIR=$(brew --prefix hdf5) cargo test -p evt3 --features hdf5
```

## Verifying Tests Actually Ran

Tests skip gracefully, and skipped tests still show as `ok`, when a fixture is
absent or the ECF compression plugin is not installed. To confirm which tests
ran versus skipped:

```bash
cargo test -p evt3 --features hdf5 -- --show-output
```

Skipped tests print a `[SKIP]` line that explains the reason.
