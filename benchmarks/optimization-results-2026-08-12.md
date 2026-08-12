# Optimization Results — 2026-08-12

## Workload

- Input: `test_data/laser.raw`
- Encoded size: 325 MiB
- Decoded CD events: 116,300,447
- Platform: Apple Silicon macOS
- Interpreter: CPython 3.11.9
- Build: release, thin LTO, one code-generation unit

The old and new Python extensions were run in alternating fresh processes.
Each result includes file read, EVT3 decode, and construction of the object
returned by the established `decode_file` call. The filesystem cache was warm.

## Compatible Full-Memory API

| Implementation | Mean time | Mean maximum RSS |
|---|---:|---:|
| Fresh build of repository `HEAD` | 3.843 s | 2.879 GB |
| Optimized working-tree build | 2.108 s | 1.178 GB |

Calculated improvement:

- Speedup: `3.843 / 2.108 = 1.82x`
- Time reduction: `1 - 2.108 / 3.843 = 45.2%`
- Measured maximum-RSS reduction: `1 - 1.178 / 2.879 = 59.1%`

All aggregate output checks matched exactly: event count, sums of `x`, `y`,
polarity and timestamp, and the final timestamp.

The raw array payload is 13 bytes per event, or 1.512 GB for this recording.
The former aligned row plus column payload required at least 29 bytes per
event, or 3.373 GB, during conversion. Maximum RSS on macOS is affected by
lazy page commitment and memory compression, so the payload calculation and
the measured RSS describe different aspects of memory use.

## Bounded-Memory API

`decode_file_batches` processed the same recording in 42 batches:

- Time including an `x.sum()` consumer for every batch: 2.505 s
- Maximum RSS: 0.227 GB
- Event count and `x` checksum: identical to full-file decoding

Compared with the previous full-memory process, measured maximum RSS decreased
by `1 - 0.227 / 2.879 = 92.1%`. Batch size is configurable. Retaining earlier
batches also retains their arrays and increases memory accordingly.

## Streaming CLI

Two alternating full-file runs wrote CSV output to `/dev/null`, so they include
CSV formatting but avoid storage-device write variance:

| Implementation | Mean time | Mean maximum RSS |
|---|---:|---:|
| Previous materialized CLI | 15.343 s | 0.940 GB |
| Streaming CLI | 9.185 s | 0.064 GB |

Calculated improvement:

- Speedup: `15.343 / 9.185 = 1.67x`
- Time reduction: `1 - 9.185 / 15.343 = 40.1%`
- Measured maximum-RSS reduction: `1 - 0.064 / 0.940 = 93.2%`

CSV and binary output from both CLI versions were also compared byte for byte
on an 8-MiB raw-file prefix and matched.

## Core Hot Path

An isolated sparse-vector prototype changed the real-file Rust decode from
approximately 1.03 s to 0.90 s in the initial audit, a reduction of about 13%.
Later absolute runs varied substantially with system memory pressure, so this
figure is reported as an observed result rather than a stable hardware claim.

The Criterion benchmark `decode_vectors/sparse_vector_masks` isolates the mask
iteration. Compared with its stored pre-optimization baseline, the final code
reduced the median time by 47.0%, which is about 1.89x the throughput. Its final
10-sample estimate was 2.18-2.26 ms, or 337-349 MiB/s.

The byte-path benchmark showed that interleaving byte conversion and event
decoding hurt throughput. The final implementation therefore converts 4,096
words into an 8-KiB stack buffer and then executes the word hot path. It avoids
the former heap scratch allocation while keeping conversion and decoding in
cache-sized phases.

## Interpretation

The main gain comes from changing the storage pipeline, not from CPU flags:
Python now writes directly into its final columns. Sparse mask iteration,
stack-batched byte conversion, capacity estimation, GIL release, thin LTO, and
one code-generation unit provide additional gains or better concurrency.
