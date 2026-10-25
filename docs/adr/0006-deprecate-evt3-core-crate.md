# 0006: Mark `evt3-core` Deprecated With A Final Re-Export Release

## Status

Accepted. Amends the `evt3-core` consequence of
[ADR 0005](./0005-crate-naming-on-crates-io.md).

## Context

ADR 0005 renamed the library from `evt3-core` to `evt3` and left `evt3-core`
0.2.0 on crates.io without further updates. The crates.io page of `evt3-core`
gives no sign of the rename. Users who find it, and dependents that still pin
it, stay on 0.2.0 and miss the fixes and performance work of 0.3 and 0.4.

crates.io has no deprecation flag. The registry shows the description and
README of the newest version, so a new version is the only place a notice can
appear.

Yanking 0.2.0 was considered and rejected. It hides the version from new
resolutions but gives no hint where the code went, and it breaks
`cargo update` for anyone who still has it in a lockfile.

## Decision

- Publish one final `evt3-core` release, 0.4.1. Its description and README say
  the crate is deprecated and name `evt3` as the replacement.
- The crate only does `pub use evt3::*;` with `evt3 = "0.4"`, so it compiles
  and docs.rs links to the real API.
- Use version 0.4.1, not 0.2.x. The API changed between 0.2 and 0.4, so a
  0.2.x re-export would break `^0.2` dependents on `cargo update`. Existing
  0.2 users are not moved silently.
- Keep the source in `compat/evt3-core/`, excluded from the workspace, so CI
  and release builds do not touch it.
- Publish it by hand once, after `evt3` 0.4.1 is on crates.io:

  ```bash
  cd compat/evt3-core
  cargo publish
  ```

## Consequences

- The crates.io page of `evt3-core` shows the deprecation notice.
- `evt3-core` 0.2.0 stays available and unchanged.
- The shim is not part of the release workflow and receives no further
  versions.
