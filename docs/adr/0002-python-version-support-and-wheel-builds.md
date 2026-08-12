# 0002: Support CPython 3.9-3.13 With Per-Interpreter Wheels

## Status

Accepted

Extended by [ADR 0004](./0004-python-314-and-free-threading.md) for CPython
3.14 and free-threaded CPython 3.14 support.

## Context

The package metadata still advertised Python 3.8 support even though Python 3.8
reached end of life in October 2024. At the same time, the release workflow
only built Python wheels with a single Python 3.11 interpreter on each OS, so
PyPI users on other supported versions had to build from source. Adding Python
3.13 support also requires newer PyO3 and `numpy` crate releases than the
project was using.

## Decision

- Set the supported Python range to CPython 3.9 through 3.13.
- Upgrade the Python bindings to `pyo3 = 0.23` and `numpy = 0.23`.
- Migrate the bindings to the current PyO3 Bound API required by that upgrade.
- Run Python CI coverage on 3.9, 3.10, 3.11, 3.12, and 3.13.
- Build release wheels per operating system and interpreter with
  `maturin build --interpreter python`.

## Consequences

- Python 3.8 users must stay on an older release or build from an older branch.
- Release CI now runs more wheel jobs because each OS builds one wheel per
  supported interpreter.
- PyPI users on supported CPython versions get prebuilt wheels instead of a
  single 3.11-only release artifact set.
