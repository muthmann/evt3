# 0007: Bounded Parallel EVT3 Decoding

## Status

Accepted. Workers are opt-in; this replaces the initial implicit dispatch.

## Context

The fixed performance target is at least 40% less elapsed time than commit
`85548072841ee92de409206883f4c130644ad4fe`: current time must be at most 0.60
times baseline time, equivalent to at least 1.667 times throughput.

The scalar changes alone miss this target on the recorded workloads. EVT3
coordinates, polarity and timestamps depend on earlier input, so parallel
pieces require their exact starting states and ordered output positions.

## Implementation

Built-in row and column sinks try parallel decoding for even byte chunks of
at least 65,536 bytes with no carried half-word. Smaller and irregular chunks,
word input, and custom sinks use the scalar decoder. Failed worker-pool setup
also falls back to scalar decoding.

The optional Cargo feature `parallel` controls the Rayon dependency. Rust
decoders default to `Parallelism::Off` even when the feature is enabled.
`Auto` and `Threads(n)` are explicit runtime choices. Concrete row/column
entry points choose the implementation; generic sink entry points always run
serially. Public `EventSink` has no decoding hooks. The Python and CLI wrappers
enable the feature and choose Auto explicitly, with `threads=` / `--threads`
allowing serial execution or a total thread limit.

An initial synchronization search skips input preceding the first TIME_HIGH.
A bounded prepass counts CD events and triggers for each part and processes
TIME_HIGH values in order, retaining the original rollover rule. Backward
searches recover each part's last Y, vector base/polarity, vector advance and
TIME_LOW/clock update. This constructs the starting state for the next part.
The last worker returns the decoder's final state for subsequent packets.

On little-endian AArch64 with NEON, each classification step counts 32
words in a 64-byte block. Population totals are widened before reduction
because a block can contain more than 255 CD events. Other targets use scalar
counting. This SIMD step accelerates
state discovery and output sizing; the event expansion remains scalar.

Each opted-in decoder lazily owns a Rayon pool. The caller processes the last
part, so a total limit of n requires at most n-1 background threads. Auto caps
the total at four and adapts the split to concurrent calls. Threads(n) uses at
most n and the available CPU count. The pool survives calls and is dropped on
policy change or decoder drop. There is no global worker pool. Auto's shared
active-call counter is a scheduling hint, not a global thread-budget guarantee.
Concurrent file callers can explicitly choose serial inner decoding.
Elapsed-time improvements include additional CPU parallelism. A smaller-job
queue was evaluated and removed in favor of this simpler work split.

The output vectors reserve the counted sizes. Workers receive disjoint slices
of spare capacity in input order. Each worker checks its written counts, and
the caller commits vector lengths only after every worker completes. The exact
CD counts justify unchecked writes within the disjoint output slices: each
ADDR_X contributes one event, and vector words contribute their masked
population count. The column slices have equal lengths established during
construction. Trigger writes retain bounds checks. No event merge or
full-recording temporary output is needed.

Scalar output uses writers bounded to 4,096 input words (8 KiB). Fresh outputs reserve counted populations
instead of the maximum twelve events per word; reusable cleared storage can
reserve the bounded maximum once. Drop commits only initialized events during
unwinding. Row output advances a pointer and derives the initialized count
from its distance to the starting pointer. These writers are private concrete
implementations. Public custom EventSink implementations receive events through
the ordinary serial state machine without participating in dispatch.

Full-file decoding reads at most 8,000,000 bytes per input batch. An initial
output-count sample supplies a heuristic file-size capacity hint before
writing the first events, avoiding a subsequent copy of that initial output.
The hint is speculative: sparse and dense recordings can grow differently,
and ordinary vector growth still handles an underestimate. Bounded file-batch
APIs retain their separate caller-selected batch sizes.

## Evidence and limitations

The tests compare discovered states and following-packet results with
single-word sequential decoding, exhaust all 65,536 word values for counting,
exercise maximum density, odd boundaries, pre-existing output, unequal public
columns and unwinding. AddressSanitizer supplements these checks.

Runtime acceptance, CPU costs and packet tail latency must be measured on the
target machine. The benchmark separates file decoding, packet decoding,
Python object creation and CSV serialization. Recording replay establishes
software timing; a connected-camera test is needed for USB scheduling,
backpressure and end-to-end frame latency.

The initial full report measured implicit parallel dispatch and does not describe
the new Off defaults. See [explicit controls](../features/parallel-decoding.md)
and [follow-up checks](../../benchmarks/parallelism-controls-2026-10-25.md).

The state-discovery design is informed by
[ParPaRaw](https://www.vldb.org/pvldb/vol13/p616-stehle.pdf), while fused
classification and bounded processing draw on the
[2026 Base64 study](https://arxiv.org/html/2610.17406) and
[2025 ARM scanning study](https://arxiv.org/html/2503.01662v1).
Their published speedups are not EVT3 measurements. See the
[research and measurement note](../../benchmarks/literature-and-runtime-2026-10-25.md).
