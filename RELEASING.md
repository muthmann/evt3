# Releasing EVT3

This document describes the release process for maintainers.

## Version Numbering

We use Semantic Versioning (SemVer): `MAJOR.MINOR.PATCH`

- **MAJOR**: Breaking API changes
- **MINOR**: New features, backward compatible
- **PATCH**: Bug fixes, backward compatible

## Pre-release Checklist

1. [ ] All tests pass on CI
2. [ ] CHANGELOG.md updated with release notes
3. [ ] Version numbers updated in:
   - `Cargo.toml` (`workspace.package.version`) — all three crates inherit this
     through `version.workspace = true`, so no member manifest needs editing
   - `Cargo.toml` (`workspace.dependencies.evt3.version` — must match)
   - `evt3-python/pyproject.toml` (maturin reads the version from here, not
     from `evt3-python/Cargo.toml`)
4. [ ] Documentation is up to date, including the `evt3` dependency example in
   `docs/features/hdf5-file-support.md`, which pins a minor version
5. [ ] Benchmarks run and results updated if needed

## Release Steps

### 1. Update Version

```bash
# Update version in Cargo.toml
# Update version in evt3-python/pyproject.toml
# Update CHANGELOG.md
git add -A
git commit -m "Release v0.x.y"
```

### 2. Create and Push Tag

```bash
git tag v0.x.y
git push origin main --tags
```

### 3. Automated Release

The GitHub Actions workflow will automatically:
- Build release binaries for all platforms
- Create a GitHub Release with binaries attached
- Publish to PyPI
- Publish the `evt3` library and then `evt3-cli` to crates.io

crates.io publishing needs the repository secret `CARGO_REGISTRY_TOKEN`.
Without it the `publish-crates` job fails and the crates stay at the previous
version.

### 4. Verify Release

- Check [GitHub Releases](https://github.com/muthmann/evt3/releases)
- Verify PyPI: `pip install evt3==0.x.y`
- Verify crates.io: `cargo install evt3-cli --version 0.x.y`
- Test installation on a clean environment

## Manual Release (if needed)

### Build CLI Binaries

```bash
# Build for current platform
cargo build --release -p evt3-cli

# Cross-compile (requires cross)
cross build --release --target x86_64-unknown-linux-gnu
cross build --release --target aarch64-unknown-linux-gnu
```

### Publish to PyPI

```bash
cd evt3-python
maturin publish --skip-existing
```

### Publish to crates.io

Publish in this order. `evt3-cli` depends on the `evt3` library by version, so it
cannot be packaged until `evt3` is in the index.

```bash
cargo publish -p evt3
cargo publish -p evt3-cli
```

## Hotfix Process

For critical bug fixes:

1. Create branch from the release tag: `git checkout -b hotfix/v0.x.y v0.x.y`
2. Apply fix and update patch version
3. Tag and release as `v0.x.(y+1)`
