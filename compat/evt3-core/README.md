# evt3-core (deprecated)

This crate was renamed to [`evt3`](https://crates.io/crates/evt3) in 0.4.0.
`evt3-core` receives no further updates.

This final release only re-exports `evt3`. Switch to the new name:

```toml
[dependencies]
evt3 = "0.4"
```

```rust
use evt3::…; // was: use evt3_core::…;
```

The API changed between `evt3-core` 0.2 and `evt3` 0.4. See the
[changelog](https://github.com/muthmann/evt3/blob/main/CHANGELOG.md) and
[ADR 0006](https://github.com/muthmann/evt3/blob/main/docs/adr/0006-deprecate-evt3-core-crate.md).
