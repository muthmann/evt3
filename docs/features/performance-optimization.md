# Decoder Performance Optimization

## Implemented Paths

- Sparse vector masks iterate only over set bits.
- Raw bytes are decoded without an intermediate word vector.
- An `EventSink` separates decoding from row or column storage.
- Python file decoding writes directly into stable NumPy columns and releases
  the GIL.
- Python, CLI, and HDF5 have bounded-memory file paths. Python offers both
  CD-only batches and `(Events, TriggerEvents)` batches.
- Release and benchmark profiles use thin LTO and one code-generation unit.
- `scripts/build-pgo.sh` creates an optional profile-guided workspace build
  from the real-file performance workload.

## Parallel Work

EVT3 timestamp and address state crosses arbitrary input chunk boundaries.
Splitting one encoded stream across threads without a state-discovery pass can
produce incorrect timestamps or coordinates. The default decoder therefore
remains sequential.

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
would require a measured two-pass boundary-state design and is not enabled by
default because it doubles the encoded-input scan.

Use `decode_file_batches_with_triggers` instead of `decode_file_batches` when
the analysis depends on external trigger edges. Trigger decoding occurs in the
same sequential core pass and does not require a second file scan.

## Measurement

Use release builds and compare the same work:

```bash
cargo bench -p evt3-core --bench decode_benchmark
python benchmarks/benchmark.py --file test_data/laser.raw --iterations 5
```

For allocation and CPU profiling, use platform tools against the release
binary. On Linux, `perf` or `cargo flamegraph` can profile the real-file test.
On macOS, use Instruments with the Time Profiler and Allocations templates.

The Python benchmark releases each result between iterations. Its full-memory
and bounded-batch results measure decode-only workflows. CLI and C++ results
include output serialization and must not be used as pure-decoder baselines.
