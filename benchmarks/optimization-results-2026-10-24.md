# Decoder Optimization — 2026-10-24

The decoder now converts little-endian byte pairs and decodes them in one pass.
It also finds the initial `TIME_HIGH` before entering the steady-state loop.
This removes the 4096-word stack-buffer copy and the initialization check on
each subsequent word. Public APIs, output layouts, timestamp handling, and
stream-boundary behavior are unchanged.

## Literature

- [Lemire and Boytsov, *Decoding billions of integers per second through
  vectorization* (2015)](https://arxiv.org/abs/1209.2137): reconstruction and
  intermediate passes matter alongside bit extraction.
- [Lemire, Kurz, and Rupp, *Stream VByte: Faster Byte-Oriented Integer
  Compression* (2018)](https://arxiv.org/html/1709.08990): branches and control
  dependencies can limit decoder throughput.
- [Prophesee EVT3 specification](https://docs.prophesee.ai/stable/data/encoding_formats/evt3.html):
  timestamp and address state must be processed in stream order.

The implementation adapts the pass-reduction and branch-reduction principles.
It does not implement the papers' compression formats or architecture-specific
SIMD kernels. Sparse masks continue to iterate over set bits. Of the first
373,266 vector words in `laser.raw`, 353,621 (94.7%) have at most two set bits.

## Method

- Baseline: decoder at commit `8554807`, with the same benchmark cases added
  before either version was measured.
- Platform: Apple Silicon, `aarch64-apple-darwin`; Rust 1.95.0, LLVM 22.1.2.
- Both versions use the Cargo benchmark profile: thin LTO, one code-generation
  unit, default features, and no additional dependencies or PGO.
- Criterion: one-second warm-up, two-second requested measurement, ten samples.
  Full-file cases exceed the requested measurement time to collect ten samples.
- Input: `laser.raw`, 341,306,299 bytes, 116,300,447 decoded CD events.
- The added vector-density cases reuse output capacity. Other cases allocate
  their output during each iteration.

The [measurement data](./optimization-results-2026-10-24.json) include input
SHA-256, Criterion intervals, and every alternating full-file process time.

## Decode-Loop Results

Times below are Criterion point estimates. The percentage is the reduction in
elapsed time computed from those estimates, not the increase in throughput.
Throughput counters in Criterion count input bytes or words, not output events.

| Workload | Before | After | Less time |
|---|---:|---:|---:|
| Word buffer, 100k single events | 600.02 µs | 446.06 µs | 25.7% |
| Byte stream, 8192-byte chunks | 722.50 µs | 482.33 µs | 33.2% |
| Byte stream, 4095-byte chunks | 650.73 µs | 482.78 µs | 25.8% |
| Sparse vectors, allocated columns | 1.2775 ms | 1.1698 ms | 8.4% |
| Empty vectors, reused columns | 561.03 µs | 503.56 µs | 10.2% |
| Sparse vectors, reused columns | 832.66 µs | 662.22 µs | 20.5% |
| Dense vectors, reused columns | 2.3876 ms | 2.3255 ms | 2.6% |
| Varying vectors, reused columns | 1.2242 ms | 1.1286 ms | 7.8% |

The reused row-layout vector cases were unchanged for empty masks and improved
by approximately 4–11% for nonempty masks. No vector-density case had a
statistically detected regression in this comparison.

## Full-File Check

The initial Criterion file measurements had large outliers. They do not support
a reliable full-file speedup claim. A second check alternated separate baseline
and optimized benchmark processes, with one warm-up per binary and ten measured
runs per workflow. Each process ran exactly one full-file case through
Criterion's `--test` mode. These timings include process startup, benchmark
setup, allocation, decoding, and result destruction.

| Full-file workflow | Before median | After median | Time change |
|---|---:|---:|---:|
| Row output | 0.909965 s | 0.899112 s | 1.2% less |
| Column output | 0.954555 s | 0.957844 s | 0.3% more |

The practical full-file result is largely unchanged. The synthetic decode-loop
gains must not be presented as equivalent Python file-decoding or CSV speedups.
Results are specific to this machine and input; other CPU architectures were
not benchmarked.

## Grouped-Vector Trial

The [OpenEB decoder](https://github.com/prophesee-ai/openeb/blob/main/hal/cpp/include/metavision/hal/decoders/evt3/evt3_decoder.h)
also suggested combining a `VECT_12, VECT_12, VECT_8` sequence into one 32-bit
mask. A portable lookahead trial reduced full-file median time by about 9%, but
made standalone empty columnar vectors about 72% slower and sparse columnar
vectors about 19% slower in Criterion point estimates. That trial was removed.
The retained implementation avoids this regression on standalone vectors.

## Verification

- Exhaustive expansion of all 4096 VECT_12 and 256 VECT_8 masks, both polarities,
  reserved VECT_8 bits, and empty-mask base advancement.
- Complete and partial 12/12/8 groups, every output bit including bit 31,
  interleaved state updates, and every byte split.
- Initial synchronization across multiple chunks, empty feeds, odd chunks,
  triggers, and timestamp rollover.
- Rust workspace tests: 34 unit tests, 16 integration tests, and two doc tests
  pass, including optional HDF5 and the installed ECF plugin.
- Rebuilt CPython 3.13 package: 22 Python tests pass; three free-threaded-only
  tests are skipped because this interpreter has the GIL enabled.
- Formatting and workspace Clippy checks pass with all targets and features.
- Full-recording CSV comparison with the C++ reference: all 116,300,447 events
  match byte for byte; both outputs have MD5
  `c406db55f255433b2fc27c7d78e381ef`.

## Reproduction

Add the current vector-density benchmark cases to the baseline before measuring
so both binaries run the same setup. Save each executable before rebuilding the
other version.

```bash
cargo bench -p evt3 --bench decode_benchmark -- \
  --warm-up-time 1 --measurement-time 2 --sample-size 10 \
  --save-baseline literature-before

# Build the changed decoder, then compare with the saved baseline.
cargo bench -p evt3 --bench decode_benchmark -- \
  --warm-up-time 1 --measurement-time 2 --sample-size 10 \
  --baseline literature-before
```

For the alternating process check, invoke each saved executable with
`--test 'decode_file/full_file$'` or
`--test 'decode_file/full_file_columns$'`, alternate their order on each pair,
and measure process wall time. Use the repository root as the working directory.
