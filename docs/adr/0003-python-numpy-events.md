# 0003: Python NumPy Events

## Status

Accepted

## Context

The Python bindings advertised NumPy-friendly event access, but the Rust
`Events` getters cloned the backing vectors into a new NumPy array on every
property access. That made common Python workflows surprising and expensive,
especially for large recordings. Analysis code also needs to construct the
same event model from existing arrays without encoding and decoding again.

## Decision

- Keep Rust decoding as the source of event data, but store decoded columns as
  Python-owned NumPy arrays inside the Rust extension object instead of Rust
  vectors exposed through clone-on-access getters.
- Expose a Python `evt3.Events` container as the public data model. It wraps
  stable NumPy arrays, provides `from_arrays(...)`, and keeps the existing
  `x`, `y`, `polarity`/`p`, `timestamp`/`t`, and sensor metadata accessors.
- Validate user-provided shape, dtype, geometry, coordinate bounds, polarity,
  and timestamp ordering at construction. Trusted decoder output avoids
  redundant validation scans.
- Keep the package focused on decoding and interoperable event arrays.
  Application-specific transport, publishing, and viewer integration belong
  to consumers of the array API.

## Consequences

- `events.x is events.x` and equivalent array identity checks hold for the
  public Python container and the lower-level Rust extension object.
- Python users can construct event containers from existing arrays and pass
  transformed arrays to their own analysis without another Rust decoding pass.
- Stable array access avoids repeated full-column allocations. Array
  normalization can still allocate when the input representation requires it.
- The public Python package has a small wrapper layer around the Rust
  extension functions so decoded results are returned as the Python
  `evt3.Events` class.
