# Parallel Decoding

The Rust library defaults to serial execution. The optional `parallel` Cargo
feature enables Rayon; enabling the feature alone does not start workers.
Fused byte conversion, vector expansion, bounded built-in output writers and
NEON output counting remain available without Rayon.

```toml
evt3 = { version = "0.5", features = ["parallel"] }
```

```rust
use evt3::{Evt3Decoder, Parallelism};

let mut decoder = Evt3Decoder::new(); // Parallelism::Off
decoder.set_parallelism(Parallelism::Threads(2));
let mut decoder = Evt3Decoder::new().with_parallelism(Parallelism::Auto);
decoder.reset(); // keeps the configured policy
```

`Off` executes on the caller. `Auto` uses at most four total decoding threads
and adapts the split to overlapping calls. `Threads(n)` limits total threads,
including the caller, to `n` and the available CPU count. Zero and one execute
serially. Without the feature all Rust settings execute serially. This differs
from Python/CLI numbers below, where zero means Auto, not `Threads(0)`.

| API | Execution policy |
|---|---|
| `decode_bytes`, `decode_file` | Configured policy, row output |
| `decode_bytes_columns`, `decode_file_columns` | Configured policy, columns |
| `decode_buffer`, `decode_buffer_into` | Always serial |
| `decode_bytes_into`, `decode_file_into` | Always serial, for every sink type |
| `EventFileReader::read_next_into` | Always serial |
| `EventFileReader::read_next_columns` | `with_parallelism` policy, default Off |
| HDF5 | Existing HDF5 path, without EVT3 worker threads |

`EventSink` exposes only reservation, length and event delivery methods.
There are no hidden decoding hooks. Built-in batch writers use private inherent
methods; foreign sinks cannot override dispatch.

Only even byte chunks of at least 65,536 bytes without a carried half-word can
use workers. A decoder creates its own pool lazily after opt-in, with at most
`n - 1` background threads; the caller processes one part. It reuses the pool
until the policy changes or the decoder drops. No global worker pool is
created. A global active-call counter helps Auto choose a smaller split; it
does not guarantee a process-wide thread budget. When files already decode
concurrently, select Off/one thread to avoid nested parallel work.

## Python and CLI

Both wrappers enable `parallel` by default and select Auto explicitly. All
public Python decoding functions and decoder/iterator constructors accept
keyword-only `threads`:

```python
import evt3
events = evt3.decode_file("recording.raw", threads=1)
decoder = evt3.Decoder(threads=2)
batches = evt3.decode_file_batches("recording.raw", threads=0)
print(evt3.PARALLEL_ENABLED)
```

```bash
evt3 recording.raw events.csv --threads 1
evt3 recording.raw events.csv --threads 2
```

Zero (default) means Auto, one disables workers, and larger values cap total
decoding threads. Existing positional arguments keep their meaning. Builds
with `--no-default-features` omit the engine: Auto runs serially, while explicit
requests greater than one fail with a clear error. Python reports build
capability as `PARALLEL_ENABLED`. For live previews, one thread removes decoder
worker scheduling; it does not bound OS scheduling or downstream latency.

## Measurement

The unified pipeline accepts `--decoder-threads 0`, `1` or `n`, independently
of `--workers` (concurrent files). Only the candidate receives this option;
the original Rust baseline retains its serial behavior. Omission uses native
Rust Off and Python/CLI Auto. JSON records the choice, defaults, build feature
and native API adapter.

```bash
evt3-python/.venv/bin/python benchmarks/benchmark.py --comparison-suite \
  --rust-baseline-ref 8554807 --decoder-threads 1 --iterations 5 \
  --output benchmarks/serial-results.json
```

The previous [full report](../../benchmarks/final-results-2026-10-25.md) measured
implicit scheduling; its numbers do not describe these new defaults. Follow-up
[control and latency checks](../../benchmarks/parallelism-controls-2026-10-25.md)
compare serial with Auto and retain every timing sample.

Decoder measurements exclude camera transfer and downstream processing.
Applications should choose a thread policy using their complete workload.
