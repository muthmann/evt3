# Explicit parallelism: control and latency checks, 25 October 2026

The implicit worker dispatch has been replaced by an optional build feature and
explicit per-decoder policy. Rust defaults to Off, Python/CLI explicitly select
Auto, and generic EventSink entry points always execute serially.
See [API and build controls](../docs/features/parallel-decoding.md).

No optimization experiment was added in this follow-up. Two five-sample checks
use the same selected implementation with `--decoder-threads 1` and `0`. Each
compares against pre-optimization Rust `85548072841ee92de409206883f4c130644ad4fe`.
The serial and Auto suites were run consecutively, not interleaved with each
other; comparisons between those columns are descriptive, not confidence bounds.
Each suite separately warms and rotates candidate/baseline measurement order.

## Complete measured selection

Twelve decode cases are selected per configuration: three concurrent-file
workloads and nine packet workloads. These are not full 22-case acceptance runs.
Packet times replay an 8-MiB in-memory payload; native outputs are reused and
Python retains all NumPy outputs. Multi-file cases use four 64-MiB prefixes of
the same scene with four file workers. File time includes reading/allocation.
The unmodified official C++ sample verifies matching CSV on 1-MiB prefixes
of every input before timing. No C++ decode-only latency ratio is inferred.

| Workload | Serial current (ms) | Auto current (ms) | Serial vs original | Auto vs original |
|---|---:|---:|---:|---:|
| `rust_rows/many_parallel` | 160.402 | 135.732 | 1.332x | 1.515x |
| `rust_columns/many_parallel` | 173.048 | 131.764 | 1.319x | 1.634x |
| `rust_stream_rows/4095` | 12.093 | 11.569 | 1.599x | 1.577x |
| `rust_stream_columns/4095` | 11.765 | 11.605 | 1.646x | 1.691x |
| `python_feed/4095` | 18.946 | 18.782 | 1.715x | 1.701x |
| `rust_stream_rows/8192` | 11.392 | 11.566 | 1.605x | 1.652x |
| `rust_stream_columns/8192` | 11.486 | 12.713 | 1.783x | 1.577x |
| `python_feed/8192` | 17.459 | 17.684 | 1.684x | 1.664x |
| `rust_stream_rows/65536` | 11.822 | 10.728 | 1.489x | 1.705x |
| `rust_stream_columns/65536` | 12.441 | 11.170 | 1.718x | 1.785x |
| `python_feed/65536` | 17.515 | 14.711 | 1.422x | 1.772x |
| `python_full/many_parallel` | 150.768 | 157.660 | 1.439x | 1.508x |

## Packet tails

Values are microseconds. p95/p99 are medians of the five per-run quantiles.
Worst is the largest call in all five runs; it is not a latency guarantee.
The first eligible Auto call includes lazy pool creation.

| API / chunk bytes | Serial p95 / p99 / worst (us) | Auto p95 / p99 / worst (us) |
|---|---:|---:|
| `rust_stream_rows/4095` | 5.96 / 6.38 / 738.92 | 6.00 / 6.62 / 1046.42 |
| `rust_stream_columns/4095` | 6.04 / 6.38 / 181.67 | 5.92 / 6.33 / 460.29 |
| `python_feed/4095` | 11.42 / 27.25 / 1127.92 | 11.37 / 26.25 / 596.58 |
| `rust_stream_rows/8192` | 11.83 / 13.33 / 84.42 | 11.92 / 17.38 / 113.88 |
| `rust_stream_columns/8192` | 11.71 / 12.79 / 51.88 | 13.12 / 33.54 / 402.62 |
| `python_feed/8192` | 20.50 / 43.50 / 1189.33 | 20.42 / 52.38 / 809.54 |
| `rust_stream_rows/65536` | 96.54 / 118.58 / 881.71 | 121.71 / 192.58 / 529.42 |
| `rust_stream_columns/65536` | 112.00 / 184.58 / 463.71 | 116.71 / 166.58 / 3044.17 |
| `python_feed/65536` | 167.67 / 242.75 / 347.08 | 186.12 / 409.88 / 5612.54 |

Auto improves some median replay times, but does not uniformly improve tail
latency or concurrent-file time. Explicit one-thread execution removes decoder
worker scheduling, not OS scheduling or unrelated system activity.

The earlier 4095-byte row outlier (about 12 ms in one call) occurred in the
scalar path: chunks below 64 KiB and odd boundaries were already ineligible
for parallel decoding. It is therefore not evidence that the decoder pool
caused that outlier. New tests verify that Auto creates no pool for those
inputs. The follow-up still records tails without claiming their cause is known.

## Verification

- Rust core without default features: 40 unit tests pass; normal dependency tree has no Rayon.
- Rust parallel APIs: 44 unit tests pass, including output/state equivalence, default Off, generic serial dispatch, odd boundaries, thread limits and policy changes.
- HDF5 + parallel release: 46 unit tests, 15 real-file/integration tests, one CLI test and two doctests pass. The old performance test is explicitly skipped.
- AddressSanitizer with parallel: 44 unit tests pass.
- Python default build: 28 passed, four skipped (three require free-threaded Python; one requires the feature-disabled build).
- Python wheel without parallel: 27 passed, five skipped (two parallel-only parameter cases and three free-threaded tests). The disabled-feature rejection test passes.
- CLI CSV output is byte-identical for feature-on threads 0/1/2/4 and feature-off 0/1; feature-off threads=2 fails clearly.
- Benchmark regressions: 18 tests pass.
- Clippy for HDF5/parallel and separately without parallel, Rust 1.83 Python compilation, formatting and diff checks pass.

Scalar decoding improvements remain available without the parallel feature.
Parallel-specific unsafe code is compiled only with that feature; existing
scalar output writers and NEON counting still use documented unsafe operations.
No new unsafe operations were needed for the configuration API.

Camera configuration, installed system packages and published releases were
unchanged. Hardware USB/backpressure and end-to-end application latency remain
unmeasured.

## Reproduce

Run this command once with 1 and once with 0:

```bash
CARGO_BUILD_JOBS=4 evt3-python/.venv/bin/python benchmarks/benchmark.py \
  --comparison-suite --decode-only --file test_data/laser.raw \
  --rust-baseline-ref 8554807 --decoder-threads 1 --iterations 5 \
  --verify-prefix-mib 1 --work-dir /private/tmp/evt3-benchmark-comparison \
  --cases 'rust_stream_*/*' 'python_feed/*' '*/many_parallel' \
  --output benchmarks/parallelism-serial-results-2026-10-25.json
```

Raw [serial samples](parallelism-serial-results-2026-10-25.json) and
[Auto samples](parallelism-auto-results-2026-10-25.json) include all outliers,
source/binary/wheel hashes, build flags and the selected policy. The
[validation snapshot](parallelism-validation-2026-10-25.json) records checks
and source hashes. The earlier [full report](final-results-2026-10-25.md)
retains its original measurements and is marked as an older scheduling design.
