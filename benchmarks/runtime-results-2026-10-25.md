# Preliminary parallel decoder measurements, 25 October 2026

**Not a final acceptance result.** These five-sample medians describe the first
complete parallel candidate (v1), not the current working tree. Background
processes were subsequently confirmed and the user reported ongoing background
work. A quiet-machine rerun is required. No physical camera was connected.

Original Rust: `85548072841ee92de409206883f4c130644ad4fe`.
Candidate tracked-diff SHA-256: `e800f264e9ac80f368783c3c06e93b0e18abb1694955ef209d5015c03d4613ca`.
The [raw JSON](runtime-parallel-v1-results-2026-10-25.json) additionally records
all Rust source hashes (including the untracked parallel module), input and
binary hashes, compiler versions and individual samples. Its `all_runtime_targets_passed`
is **false**. No results were chosen from later partial runs to fill this table.

The target is at least **40% less elapsed time**, or at least 1.667x throughput.
CSV includes formatting and process startup. Native and Python file output
includes reading, decoding and allocation; validation is outside the timer.
Packet rows are total time for an 8-MiB in-memory payload replay, not one packet.

## File and packet decoding

| Workload | Original Rust/Python (ms) | v1 (ms) | Throughput ratio | Less time | 40% gate |
|---|---:|---:|---:|---:|---|
| `rust_rows/small` | 21.737 | 7.289 | 2.982x | 66.5% | Pass |
| `rust_rows/large` | 897.636 | 325.455 | 2.758x | 63.7% | Pass |
| `rust_rows/many_sequential` | 699.442 | 232.595 | 3.007x | 66.7% | Pass |
| `rust_rows/many_parallel` | 200.388 | 142.901 | 1.402x | 28.7% | Miss |
| `rust_columns/small` | 22.618 | 6.899 | 3.279x | 69.5% | Pass |
| `rust_columns/large` | 938.386 | 291.861 | 3.215x | 68.9% | Pass |
| `rust_columns/many_sequential` | 733.680 | 222.220 | 3.302x | 69.7% | Pass |
| `rust_columns/many_parallel` | 203.176 | 125.820 | 1.615x | 38.1% | Miss |
| `rust_stream_rows/4095` | 17.803 | 12.410 | 1.435x | 30.3% | Miss |
| `rust_stream_columns/4095` | 19.525 | 12.325 | 1.584x | 36.9% | Miss |
| `python_feed/4095` | 31.774 | 19.859 | 1.600x | 37.5% | Miss |
| `rust_stream_rows/8192` | 17.944 | 12.371 | 1.450x | 31.1% | Miss |
| `rust_stream_columns/8192` | 19.588 | 12.454 | 1.573x | 36.4% | Miss |
| `python_feed/8192` | 28.221 | 19.687 | 1.433x | 30.2% | Miss |
| `rust_stream_rows/65536` | 17.810 | 10.427 | 1.708x | 41.5% | Pass |
| `rust_stream_columns/65536` | 19.276 | 10.257 | 1.879x | 46.8% | Pass |
| `python_feed/65536` | 24.429 | 12.724 | 1.920x | 47.9% | Pass |
| `python_full/small` | 24.076 | 8.489 | 2.836x | 64.7% | Pass |
| `python_full/large` | 932.161 | 273.790 | 3.405x | 70.6% | Pass |
| `python_full/many_sequential` | 737.282 | 234.882 | 3.139x | 68.1% | Pass |
| `python_full/many_parallel` | 213.887 | 119.812 | 1.785x | 44.0% | Pass |
| `python_batches/large` | 1240.005 | 339.917 | 3.648x | 72.6% | Pass |

The small input is an 8-MiB prefix; the full recording is approximately
325.5 MiB. Multi-file cases use four separate 64-MiB prefixes of the same
recording, sequentially or with four file workers. They do not cover four
independent scenes. Current v1 decoding also uses shared background workers.

## Same-work comparison with the official Prophesee C++ sample

The vendored standalone sample is unmodified and built with Apple Clang 21,
`-O3 -DNDEBUG -std=c++17`. This is not a measurement of every full-SDK mode.
All three implementations write CSV to the null device. No C++ decode-to-NumPy
or packet ratio is inferred from this CSV comparison.

| CSV workload | Prophesee C++ (s) | Original Rust (s) | v1 Rust (s) | v1 vs C++ | v1 vs original Rust |
|---|---:|---:|---:|---:|---:|
| `csv/small` | 0.278134 | 0.182272 | 0.159229 | 1.747x | 1.145x |
| `csv/large` | 11.165751 | 6.956339 | 6.280696 | 1.778x | 1.108x |
| `csv/many_sequential` | 8.847141 | 5.577172 | 5.036871 | 1.756x | 1.107x |
| `csv/many_parallel` | 2.282493 | 1.451583 | 1.339274 | 1.704x | 1.084x |

CSV byte equality was verified on an 8-MiB prefix of each input. Native
file comparisons also check full-output counts, sums and an order-sensitive CD
hash. Python checks counts, sums and timestamp edges; these checks are weaker
than comparing every Python array element.

## 64-KiB packet latency

Each cell is the median of the five per-run latency statistics, in microseconds.
Each run retains its own raw quantiles in the JSON. These are synchronous
decode-call latencies; they exclude USB transfer and downstream application work.

| API | Original p50 / p95 / p99 / max (us) | v1 p50 / p95 / p99 / max (us) |
|---|---:|---:|
| `rust_stream_rows/65536` | 137.62 / 150.96 / 179.04 / 193.08 | 78.67 / 112.38 / 128.96 / 155.08 |
| `rust_stream_columns/65536` | 148.29 / 168.50 / 202.71 / 324.33 | 72.79 / 114.38 / 185.29 / 249.79 |
| `python_feed/65536` | 183.75 / 258.46 / 319.00 / 350.25 | 92.75 / 136.25 / 204.33 / 301.50 |

A passing median does not establish 40% lower p99 latency. In particular,
column-output p99 improved much less than its overall replay time.

## Remaining acceptance work

Native concurrent files, 4095/8192-byte packet workloads and final current-code
measurements remain open in this historical snapshot. Physical USB,
backpressure, and end-to-end application latency require separate hardware
tests and are not established by decoder replay measurements.

See the [literature and reproduction note](literature-and-runtime-2026-10-25.md)
for sources, rejected experiments and the command for a later full run.
