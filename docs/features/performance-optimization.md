# Decoder Performance Optimization

Parallel execution now requires the optional `parallel` Cargo feature and an
explicit policy. Rust defaults to Off; Python and CLI explicitly select Auto.
See [parallelism controls](parallel-decoding.md) for `Parallelism`, `threads=`
and `--threads`, and the distinction between concrete and generic APIs.

## Implemented Paths

- Sparse vector masks iterate only over set bits.
- Raw byte pairs are converted and decoded in one pass, without an intermediate
  word vector or stack-buffer copy.
- Initial `TIME_HIGH` synchronization runs before the steady-state word loop,
  so initialized streams do not repeat the initialization check per word.
- Complete BASE/12/12/8 groups share classification and mask expansion;
  partial groups retain scalar handling.
- An `EventSink` separates decoding from row or column storage.
- Bounded writers reserve counted output for fresh buffers and reuse capacity
  for live outputs. Large even byte chunks can decode into disjoint ranges
  using explicitly enabled decoder workers after exact state discovery.
- Python file decoding writes directly into stable NumPy columns and releases
  the GIL.
- Python, CLI, and HDF5 have bounded-memory file paths. Python offers both
  CD-only batches and `(Events, TriggerEvents)` batches.
- Release and benchmark profiles use thin LTO and one code-generation unit.
- `scripts/build-pgo.sh` creates an optional profile-guided workspace build
  from the real-file performance workload.

## Literature And Design

[Lemire and Boytsov (2015)](https://arxiv.org/abs/1209.2137) show that fast
integer decoding depends on the complete reconstruction pipeline, including
intermediate passes and output costs.
[Lemire, Kurz, and Rupp (2018)](https://arxiv.org/html/1709.08990) describe how
branches and dependencies in control processing can limit throughput. These
principles motivate fusing byte conversion with decoding and moving initial
synchronization outside the main loop.

This is an adaptation of those principles to EVT3, not an implementation of
SIMD-BP128 or Stream VByte. Their encodings differ from EVT3. The
[EVT3 specification](https://docs.prophesee.ai/stable/data/encoding_formats/evt3.html)
requires sequential timestamp, address, polarity, and vector-base updates.
Event expansion uses portable scalar set-bit iteration: in the first million
words of `laser.raw`, 94.7% of vector masks contain at most two events. The
parallel path uses Rayon and an AArch64 NEON counting prepass
where supported. Other targets retain scalar counting. The
[October 25 research note](../../benchmarks/literature-and-runtime-2026-10-25.md)
connects the newer implementation to 2025–2026 parsing literature.

The [October 2026 measurements](../../benchmarks/optimization-results-2026-10-24.md)
compare the changes against the same compiler and build settings. They also
cover empty, sparse, dense, and varying vector masks in both output layouts.

## Parallel Work

EVT3 state crosses input boundaries. Opted-in concrete byte paths
therefore discover exact starting states before splitting even chunks of at
least 64 KiB. Each decoder lazily owns its pool; the calling thread processes
the last part. Auto caps the total at four and adapts to overlapping calls.
Threads(n) caps the total including the caller. Smaller chunks, carried half-words,
word-buffer input and custom sinks use the scalar path. See
[ADR 0007](../adr/0007-bounded-parallel-decoding.md) for state recovery, output
initialization, fallback behavior and the CPU-parallelism tradeoff.

Python releases the GIL during complete-file and batch reads. Applications can
safely decode independent recordings with `concurrent.futures.ThreadPoolExecutor`:

```python
from concurrent.futures import ThreadPoolExecutor
import evt3

with ThreadPoolExecutor() as executor:
    recordings = list(executor.map(evt3.decode_file, paths))
```

Batch consumers can also submit analysis of completed batches to worker
threads or processes while the next batch is decoded. Parallel in-file decode
uses the boundary-state design above. For concurrent files, select `threads=1`
when nested decoder parallelism is undesirable.

Use `decode_file_batches_with_triggers` instead of `decode_file_batches` when
the analysis depends on external trigger edges. Trigger decoding occurs in the
same sequential core pass and does not require a second file scan.

## Measurement

Use release builds and compare the same work:

```bash
cargo bench -p evt3 --bench decode_benchmark
python benchmarks/benchmark.py --file test_data/laser.raw --iterations 5
```

For a single pipeline covering small files, the complete recording, sequential
and parallel multi-file decoding, and Python output, run:

```bash
evt3-python/.venv/bin/python benchmarks/benchmark.py --comparison-suite \
  --rust-baseline-ref 8554807 --iterations 5 \
  --output benchmarks/comparison-results.json
```

This builds the official standalone Prophesee C++ sample, the selected
pre-change Rust commit, and the current working tree. It reports speedups only
within matching output workflows. The C++ sample writes CSV and has no
equivalent Python decode-to-NumPy mode. See the
[suite documentation](../../benchmarks/README.md#unified-comparison-suite)
for input sizes, verification, and timing boundaries.

For a Criterion comparison, save a baseline before changing the decoder and
run the same benchmark afterward:

```bash
cargo bench -p evt3 --bench decode_benchmark -- --save-baseline before
cargo bench -p evt3 --bench decode_benchmark -- --baseline before
```

The `decode_vector_density` group reuses output storage to isolate decoding
from allocation. The existing full-file and byte-chunk groups include output
allocation. Throughput is in input bytes or words, not decoded events, because
one vector word can emit multiple events.

For allocation and CPU profiling, use platform tools against the release
binary. On Linux, `perf` or `cargo flamegraph` can profile the real-file test.
On macOS, use Instruments with the Time Profiler and Allocations templates.

The Python benchmark releases each result between iterations. Its full-memory
and bounded-batch results measure decode-only workflows. CLI and C++ results
include output serialization and must not be used as pure-decoder baselines.

Before the newer group/output/parallel changes, the [October 25 comparison](../../benchmarks/comparison-results-2026-10-25.md),
the complete-recording CSV median is 6.815 s for current Rust, 6.853 s for the
pre-change Rust baseline, and 11.096 s for the `-O3 -DNDEBUG` C++ sample. That is
1.63x the C++ throughput and 1.006x the pre-change Rust throughput. The Python
full-memory medians are 0.935 s current versus 0.936 s pre-change. Those earlier
synthetic-loop improvements did not establish substantial real-file gains.
The newer runtime suite records an explicit 40%-less-time gate for each decode
case, along with native and Python packet-latency distributions. Its
`full_runtime_acceptance_passed` field determines whether every required runtime
case passed; a passing `--cases` selection does not establish completion.
The [final comparison](../../benchmarks/final-results-2026-10-25.md) records the
selected implementation's gains and the workloads that miss the 40% target.
Physical USB/backpressure and preview latency remain unmeasured without a
connected camera.
The [August report](../../benchmarks/optimization-results-2026-08-12.md) retains
the earlier storage-pipeline and memory measurements.
