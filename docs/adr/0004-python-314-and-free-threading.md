# 0004: Support CPython 3.14 And Free-Threaded Builds

## Status

Accepted

## Context

The Python package supports CPython through 3.13. CPython 3.14 requires a
newer PyO3 release, and Python 3.14 also makes the free-threaded interpreter a
supported runtime variant. Importing an extension that does not declare
free-threading support silently enables the GIL for the process.

The bindings expose NumPy arrays and stateful decoders. Declaring the module as
free-threading compatible therefore requires both an audit of the exposed Rust
types and runtime tests that exercise concurrent access.

## Decision

- Support GIL-enabled and free-threaded CPython 3.14.
- Upgrade the bindings to PyO3 0.29 and `numpy` 0.29, with Rust 1.83 as the
  minimum Rust version for the Python extension.
- Declare `gil_used = false` explicitly on the extension module.
- Replace the removed `Python::allow_threads` API with `Python::detach`.
- Test the complete synthetic Python suite on CPython 3.14 and add a dedicated
  free-threaded 3.14 CI job that checks import behavior, independent concurrent
  decoding, and concurrent read access to decoded events.
- Publish interpreter-specific wheels for both CPython 3.14 variants.

## Consequences

- Importing `evt3` does not re-enable the GIL in free-threaded CPython 3.14.
- Independent decoder instances can be used from concurrent Python threads.
- Shared decoded arrays are supported for concurrent read-only access. As with
  NumPy generally, callers must synchronize concurrent writes themselves.
- A single stateful `Decoder` or file iterator must not be advanced by several
  threads at the same time. PyO3 rejects overlapping mutable borrows.
- Release CI builds more wheels because free-threaded CPython has a distinct
  ABI on Python 3.14.
