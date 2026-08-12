# 0005: Publish The Rust Library As `evt3` And The CLI As `evt3-cli`

## Status

Accepted

## Context

The workspace published one crate, `evt3-core`, and kept the CLI unpublished.
Three problems followed from this.

The CLI could not be installed the usual way. `cargo install` had no target, so
Rust users who found the project had to clone and build it.

The name `evt3` was unclaimed on crates.io. It is the term users search for,
and it matches the PyPI package name, where `evt3` is the Python library.

The `evt3-cli` manifest declared six keywords. crates.io permits five, so the
crate could not be published at all.

Giving the short name to the CLI was considered and rejected. A binary-only
crate has no lib target, so `cargo add evt3` succeeds silently, writes the
dependency into `Cargo.toml`, and only fails later at compile time with
`E0433`. The compiler's help text then suggests running `cargo add evt3`, which
the user has already done. Registry names cannot be reassigned once taken, so
the choice had to be made before the first publish.

## Decision

- Publish the decoder library as `evt3`. The directory stays `evt3-core`; only
  the package and lib target are renamed, so the import path becomes
  `use evt3::…`.
- Publish the CLI as `evt3-cli`. It continues to install a binary named `evt3`.
- Rename the `evt3-python` lib target to `evt3_python`. It previously used
  `evt3`, which would have been ambiguous against the `evt3` dependency.
  maturin takes the Python module name from `tool.maturin.module-name`, so the
  built module is unchanged.
- Declare the internal dependency once in `[workspace.dependencies]` with an
  explicit version. crates.io rejects a publish whose path dependency carries
  no version.
- Publish `evt3` before `evt3-cli`. The CLI depends on the library by version
  and cannot be packaged until the library is in the index.

## Consequences

- `cargo add evt3` gives the library, `cargo install evt3-cli` gives the CLI,
  and `pip install evt3` gives the Python package. `evt3` means the library on
  both registries.
- This is a breaking change for Rust library users. `use evt3_core::…` becomes
  `use evt3::…`, and the dependency name changes.
- `evt3-core` 0.2.0 stays on crates.io and is not updated further. At 199
  downloads the cost of leaving it behind is low, and registry names cannot be
  reused later.
- The workspace version must be updated in two places on release:
  `workspace.package.version` and `workspace.dependencies.evt3.version`.
- Releases publish two crates instead of one, so a partial failure can leave
  the library published and the CLI not. The release workflow treats an
  already-published version as success so a re-run recovers.
