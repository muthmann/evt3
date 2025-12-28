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
   - `Cargo.toml` (workspace version)
   - `evt3-python/pyproject.toml`
4. [ ] Documentation is up to date
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
- Publish to crates.io (if configured)

### 4. Verify Release

- Check [GitHub Releases](https://github.com/your-username/evt3/releases)
- Verify PyPI: `pip install evt3==0.x.y`
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

```bash
cargo publish -p evt3-core
cargo publish -p evt3-cli
```

## Hotfix Process

For critical bug fixes:

1. Create branch from the release tag: `git checkout -b hotfix/v0.x.y v0.x.y`
2. Apply fix and update patch version
3. Tag and release as `v0.x.(y+1)`
