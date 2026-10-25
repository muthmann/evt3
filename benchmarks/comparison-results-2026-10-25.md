# Decoder Comparison — 2026-10-25

The current Rust decoder is 1.56–1.66x as fast as the official Prophesee
standalone C++ sample for matched CSV workloads in this run. The latest Rust
changes add approximately 0–2% end-to-end throughput over the pre-change Rust
version. They do not deliver a 26–33% full-file or Python speedup.

## Baselines And Method

- Prophesee: unmodified [official standalone EVT3 sample](https://github.com/prophesee-ai/openeb/blob/main/standalone_samples/metavision_evt3_raw_file_decoder/metavision_evt3_raw_file_decoder.cpp),
  freshly compiled with `-O3 -DNDEBUG -std=c++17`. The vendored source was
  checked against upstream and matches. This comparison does not benchmark the
  separate Metavision SDK decoder.
- Pre-change Rust: `85548072841ee92de409206883f4c130644ad4fe`, the version
  before this chat's literature-guided changes. The earlier August 2026
  columnar/streaming optimizations are already included in this baseline.
- Current Rust: the working tree, including the one-pass conversion and
  initialization-check changes. Both Rust versions use release builds, the
  same dependency lockfile, and the same CPython 3.13.5 interpreter.
- Five measured runs per implementation and workload after one warm-up each.
  Implementation order rotates. Tables use medians; warm filesystem cache.
- Small input: 8 MiB recording prefix. Large input: complete 325.5 MiB
  `laser.raw`, with 116,300,447 CD events.
- Multi-file input: four separate 64 MiB files containing the same recording
  prefix, 256 MiB total and 91,444,056 events. Parallel cases use four workers.
  They test independent file decoding, not four different camera scenes.

Raw samples, ranges, hashes, compiler details, and verification data are in
[the JSON report](./comparison-results-2026-10-25.json).

## Matched CSV Export

All implementations read the input, decode CD events, format CSV, and write
it to the null device. Timings include process startup and file reads; they
do not measure writing a large CSV file to physical storage.

| Workload | Prophesee C++ | Pre-change Rust | Current Rust | Current vs C++ | Current vs pre-change |
|---|---:|---:|---:|---:|---:|
| Single 8 MiB file | 0.2746 s | 0.1777 s | 0.1764 s | 1.557x | 1.007x |
| Single 325.5 MiB file | 11.0963 s | 6.8533 s | 6.8147 s | 1.628x | 1.006x |
| 4 × 64 MiB, sequential | 9.0650 s | 5.4652 s | 5.4528 s | 1.662x | 1.002x |
| 4 × 64 MiB, 4 workers | 2.2792 s | 1.4201 s | 1.3951 s | 1.634x | 1.018x |

## Python Decode To NumPy

Python imports and checksum verification are outside these timers. Timings
include file read, Rust decoding, and construction of the returned arrays and
objects. Multi-file full-memory cases retain all returned arrays until timing
ends, in both sequential and parallel modes.

| Workload | Pre-change Rust/Python | Current Rust/Python | Current vs pre-change |
|---|---:|---:|---:|
| Single 8 MiB file | 0.023622 s | 0.023165 s | 1.020x |
| Single 325.5 MiB file | 0.936163 s | 0.934982 s | 1.001x |
| 4 × 64 MiB, sequential | 0.733338 s | 0.727262 s | 1.008x |
| 4 × 64 MiB, 4 workers | 0.193665 s | 0.193288 s | 1.002x |

Bounded Python batches on the complete recording, including a checksum
consumer, take 1.230686 s before and 1.228246 s after
(1.002x). This is a different workload from returning all arrays.

There is no matching C++ decode-to-NumPy ratio: the unmodified official sample
exports CSV. Dividing its CSV time by Python's decode-only time would compare
different work.

## Multi-File Scaling

- CSV: four workers take 1.3951 s versus 5.4528 s sequentially, 3.91x the aggregate throughput.
- Python NumPy: four workers take 0.1933 s versus 0.7273 s sequentially, 3.76x the aggregate throughput.

This parallelism was already available before the latest changes. It runs
separate decoders for separate files. One encoded file remains sequential.

## Interpretation And Verification

The previous 26–33% time reduction belongs to synthetic byte-loop benchmarks.
The matched real-file measurements show that the practical gain from that
change is small. Differences of a few percent here should not be treated as
stable hardware guarantees: only five samples and one source recording were
used, and the result does not establish behavior on other CPU architectures.

- All three CLIs produced byte-identical CSV on an 8 MiB prefix of each input.
- Complete Python outputs matched on counts, x/y/polarity/timestamp sums, and
  first/last timestamps on every warm-up and measured run.
- The suite completed all nine workload cases.
- Eleven regression tests cover ordering, speedup calculations, complete-word
  input prefixes, failed commands, incorrect output, batch aggregation, and
  report snapshots. The tests are also included in CI.

## Reproduce

```bash
evt3-python/.venv/bin/python benchmarks/benchmark.py --comparison-suite \
  --rust-baseline-ref 8554807 --iterations 5 \
  --output benchmarks/comparison-results-2026-10-25.json
```

Use `--multi-files` to substitute different real recordings. See the
[suite documentation](./README.md#unified-comparison-suite) for build
requirements and fixture options.
