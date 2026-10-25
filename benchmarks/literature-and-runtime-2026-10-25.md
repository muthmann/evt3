# EVT3 runtime investigation, 25 October 2026

This note records the optimization experiments before the API follow-up.
Parallelism now has explicit feature/runtime controls, with Rust Off by default
and generic sinks always serial. See [the controls](../docs/features/parallel-decoding.md)
and [follow-up checks](parallelism-controls-2026-10-25.md). Earlier shared-pool
measurements below describe their recorded source snapshots.

## Target and baseline

The user's target means **at least 40% less elapsed time**, not merely 40%
greater throughput. For each accepted workload, `current / original <= 0.60`.
The pre-optimization Rust baseline remains commit
`85548072841ee92de409206883f4c130644ad4fe`; it is rebuilt with the same Rust
toolchain and interpreter. The Prophesee baseline is its unmodified official
standalone C++ sample, built with `-O3 -DNDEBUG -std=c++17`.

## Literature and implementation choices

| Primary source | Applicable idea | EVT3 application and boundary |
|---|---|---|
| Lemire, [Fast Forgiving Base64 Decoding with SIMD Instructions](https://arxiv.org/html/2610.17406), TACO/preprint, October 2026 | Fuse classification with decoding; bound temporary storage; handle exceptional blocks locally | Classify complete BASE/12/12/8 groups together and decode their 32 validity bits. Partial groups follow the ordinary state machine. Base64's published throughput does not predict EVT3 throughput. |
| Lemire, [Scanning HTML at Tens of Gigabytes per Second on ARM Processors](https://arxiv.org/html/2503.01662v1), Software: Practice and Experience, 2025 | Classify several input elements together and amortize mask handling on ARM | NEON counts 32 EVT3 words per 64-byte block and identifies blocks containing TIME_HIGH; scalar processing handles the few matching time words. Totals are widened before reduction. |
| Koekkoek and Lemire, [Parsing Millions of DNS Records per Second](https://arxiv.org/html/2411.12035v1), Software: Practice and Experience, 2025 | Separate structural discovery from reconstruction while keeping working storage bounded | Count exact output ranges and recover chunk states before parallel reconstruction, retaining bounded file batches. |
| Stehle and Jacobsen, [ParPaRaw: Massively Parallel Parsing of Delimiter-Separated Raw Data](https://www.vldb.org/pvldb/vol13/p616-stehle.pdf), PVLDB, 2020 | Determine input context before independent parallel parsing | Derive each EVT3 chunk's timestamp epoch, Y and vector state. This is a CPU adaptation of the context-discovery principle; the GPU parser is not used. |
| Prophesee, [EVT3 encoding](https://docs.prophesee.ai/stable/data/encoding_formats/evt3.html) and [decoder configuration](https://docs.prophesee.ai/stable/data/streaming_decoding/configuration.html) | Preserve state transitions and distinguish validation modes | Keep the baseline's decoding semantics. Results for the standalone sample do not establish performance against every SDK configuration. |

The important inference is that independent output layout work and state
discovery offer more headroom than repeated small changes to the scalar
dispatch loop. The implementation uses a SIMD counting prepass on supported
AArch64 targets, followed by scalar event reconstruction in disjoint output
ranges. It preserves output order and passes the final state to the next
packet. See [ADR 0007](../docs/adr/0007-bounded-parallel-decoding.md).

## Experiments

Fused 32-bit vector masks reduced recorded-payload scalar decoding time, while
bounded output writers substantially helped columns. Wide packed tag checks
improved that path further. These scalar changes alone still missed the 40%
runtime target in the first complete native/Python run.

Experiments with X-run coalescing, explicit hot-branch hints, a different
set-bit clearing operation, and a front-of-loop group probe did not provide
reliable additional gains and were removed. An oversized output reservation
also regressed small Python feeds; fresh outputs now use counted budgets.

The first complete parallel run reached the median-time target for 64-KiB
packets and sequential full-file decoding. Native concurrent files and smaller
packets still missed it. The [v1 report](runtime-results-2026-10-25.md) remains
a measurement of that earlier source snapshot.

Timing work stopped while the user reported background work. After the user
confirmed that the machine was sufficiently free, complete and focused runs
compared the follow-up changes. The selected code includes a local state guard,
earlier file reservation, an 8,000,000-byte file read buffer, 8-KiB bounded
scalar writers, cursor-based row writes, and wider NEON counting. A smaller-job
queue, paired ADDR_X handling, explicit 16-bit byte loads, and iterator-based
parallel row output were evaluated and removed. The selected pool divides input
into two to four parts according to active callers; the caller decodes the last
part. The shared pool uses additional CPU parallelism.

The user then requested an end to optimization and completion with the best
available result. The selected implementation is frozen for the
[final full comparison](final-results-2026-10-25.md). This ends the optimization
work at the user's request; it does not imply that every workload meets the
original 40%-less-time target. All final table entries come from one complete
run of that source snapshot. No physical camera was available.

## Reproduction and reporting

```bash
evt3-python/.venv/bin/python benchmarks/benchmark.py --comparison-suite \
  --rust-baseline-ref 8554807 --iterations 5 \
  --work-dir /private/tmp/evt3-benchmark-comparison \
  --output benchmarks/final-results-2026-10-25.json
```

Use `--decode-only` to skip CSV timings while retaining C++ prefix equality
checks. The suite records native rows and columns for small/large/several
files, Rust packet replay and Python Decoder.feed at 4095/8192/65536 bytes,
Python full-memory output, and bounded batches with a checksum consumer.
Packet results include p50/p95/p99/max call latency. All native file outputs
also contribute an order-sensitive event hash after timing.

The JSON contains individual samples, fixed baseline commit, source and binary
hashes, input hashes, toolchain settings, dependency locks and explicit
40%-runtime gates. `status: complete` means measurement completed;
`all_runtime_targets_passed` reports whether the measured cases met the target;
`full_runtime_acceptance_passed` also requires every runtime case to be present.
An exploratory run that misses gates remains useful evidence and does not
establish completion of the optimization goal.

The available recording is `laser.raw`. Four generated file prefixes exercise
multiple decoders but contain the same scene. Generality across scenes and
physical live-camera behavior require further recordings and hardware tests.
