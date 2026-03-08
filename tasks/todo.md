# HDF5 File Support

## Plan

- [x] Add feature-gated HDF5 support to `evt3-core` without changing the public API.
- [x] Extend `Evt3Decoder::decode_file` to dispatch `.h5` and `.hdf5` inputs and preserve the existing `.raw` path.
- [x] Add internal HDF5 decoding helpers for geometry, CD events, and trigger events.
- [x] Propagate the `hdf5` feature through `evt3-cli` and `evt3-python`.
- [x] Add tests for feature-disabled error handling and feature-enabled HDF5 decoding.
- [x] Add feature and ADR documentation, plus feature index updates.
- [x] Run default and `hdf5`-enabled verification and record results.

## Notes

- The working tree already contains an unrelated local change in `evt3-core/src/decoder.rs`. Preserve it while applying the HDF5 changes.
- Replace the external `laser.hdf5` fixture dependency from the plan with self-generated HDF5 test fixtures so verification stays local and reproducible.

## Review

- Implemented extension-based HDF5 dispatch in `Evt3Decoder::decode_file` with a feature-disabled fast-fail for `.h5` and `.hdf5` inputs.
- Added an internal `hdf5_decoder` module that reads `geometry`, `CD/events`, and `EXT_TRIGGER/events` datasets into the existing decode result types.
- Used the maintained `hdf5-metno` package aliased as `hdf5` instead of the originally planned `hdf5 = 0.8`, because the latter does not build against the local Homebrew HDF5 `1.14.5`.
- Replaced the external binary fixture dependency from the plan with self-generated HDF5 test fixtures for deterministic in-repo verification.
- Verification results:
  - `cargo test -p evt3-core --no-run`
  - `cargo test -p evt3-core test_hdf5_requires_feature -- --nocapture`
  - `cargo test -p evt3-core test_decode_real_file -- --nocapture`
  - `HDF5_DIR=/opt/homebrew/opt/hdf5 cargo test -p evt3-core --features hdf5 --no-run`
  - `HDF5_DIR=/opt/homebrew/opt/hdf5 cargo test -p evt3-core --features hdf5 test_hdf5_ -- --nocapture`
  - `HDF5_DIR=/opt/homebrew/opt/hdf5 cargo test -p evt3-core --features hdf5 test_decode_real_file -- --nocapture`
  - `HDF5_DIR=/opt/homebrew/opt/hdf5 cargo check -p evt3-cli --features hdf5`
  - `HDF5_DIR=/opt/homebrew/opt/hdf5 cargo check -p evt3-python --features hdf5`

## Follow-Up: Skip Visibility And Plugin Docs

### Plan

- [x] Make real-data test skips visible under `--show-output` with a consistent `[SKIP]` prefix.
- [x] Document how skipped tests appear in `evt3-core/test_data/README.md`.
- [x] Replace the vague ECF plugin note in the HDF5 feature brief with concrete installation and verification steps.
- [x] Add the `--show-output` guidance to the top-level README HDF5 section.
- [x] Run verification for skip visibility and the default test suite, then record the results.

### Review

- Added a shared `print_skip` helper and converted the real-data skip paths to `println!` with a `[SKIP]` prefix so they are visible under `cargo test -- --show-output`.
- Added a file-level note in the integration test module explaining that skipped real-data tests still report `ok` unless `--show-output` is used.
- Documented skip verification in `evt3-core/test_data/README.md`.
- Replaced the vague ECF plugin note with concrete standalone `prophesee-ai/hdf5_ecf` build and verification instructions in the HDF5 feature brief.
- Added a top-level README note that points users at `--show-output` and the detailed plugin guide.
- Verification results:
  - `HDF5_DIR=/opt/homebrew/opt/hdf5 cargo test -p evt3-core --features hdf5 test_hdf5_real_file_ -- --show-output`
  - `cargo test -p evt3-core`
