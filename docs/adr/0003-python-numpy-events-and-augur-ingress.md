# 0003: Python NumPy Events And Augur Ingress

## Status

Accepted

## Context

The Python bindings advertised NumPy-friendly event access, but the Rust
`Events` getters cloned the backing vectors into a new NumPy array on every
property access. That made common Python workflows surprising and expensive,
especially for large recordings.

The Augur handoff also needs a small Python-side connector that can publish
decoded or transformed event arrays into a running Augur session without
requiring users to know the packed transport format.

## Decision

- Keep Rust decoding as the source of event data, but store decoded columns as
  Python-owned NumPy arrays inside the Rust extension object instead of Rust
  vectors exposed through clone-on-access getters.
- Expose a Python `evt3.Events` container as the public data model. It wraps
  stable NumPy arrays, provides `from_arrays(...)`, and keeps the existing
  `x`, `y`, `polarity`/`p`, `timestamp`/`t`, and sensor metadata accessors.
- Implement `evt3.augur` in Python. The connector validates array shape,
  dtype, geometry, and timestamp ordering, then packs bounded chunks into
  Augur's 14-byte `packed_xypt_v1` wire format.
- Use loopback TCP protocol v1 for the first ingress stage. Shared memory,
  Arrow, and external timeline backends remain future work.

## Consequences

- `events.x is events.x` and equivalent array identity checks now hold for the
  public Python container and the lower-level Rust extension object.
- Python users can construct event containers from existing arrays and publish
  transformed arrays without going back through Rust.
- The connector performs a bounded copy per chunk during publication. It avoids
  a full packed dataset copy by design.
- The public Python package now has a small wrapper layer around the Rust
  extension functions so decoded results are returned as the Python
  `evt3.Events` class.
