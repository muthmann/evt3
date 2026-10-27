//! Bounded parallel decoding after discovering each chunk's input state.
//!
//! Output ranges are disjoint safe slices of spare capacity. Only their fully
//! initialized prefixes become visible after every worker has completed.
use super::counts::{block_counts, scalar_counts, Counts};
use super::*;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

static ACTIVE_CALLS: AtomicUsize = AtomicUsize::new(0);

struct ActiveCall;
impl Drop for ActiveCall {
    fn drop(&mut self) {
        ACTIVE_CALLS.fetch_sub(1, Ordering::Relaxed);
    }
}

fn available_cpus() -> usize {
    static CPUS: OnceLock<usize> = OnceLock::new();
    *CPUS.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    })
}

const MIN_PARALLEL_BYTES: usize = 65_536;

struct Part<'a> {
    bytes: &'a [u8],
    decoder: Evt3Decoder,
    counts: Counts,
}

impl Evt3Decoder {
    fn count_and_advance(&mut self, bytes: &[u8], advance: bool) -> Counts {
        let mut total = Counts::default();
        let mut blocks = bytes.chunks_exact(64);
        for block in &mut blocks {
            let counts = block_counts(block.try_into().unwrap());
            total.cd = total
                .cd
                .checked_add(counts.cd)
                .expect("event count overflow");
            total.triggers = total
                .triggers
                .checked_add(counts.triggers)
                .expect("trigger count overflow");
            if advance && counts.high {
                for word in block.chunks_exact(2) {
                    let word = u16::from_le_bytes([word[0], word[1]]);
                    if word >> 12 == 8 {
                        self.process_time_high(word);
                    }
                }
            }
        }
        let tail = blocks.remainder();
        let counts = scalar_counts(tail);
        total.cd = total
            .cd
            .checked_add(counts.cd)
            .expect("event count overflow");
        total.triggers = total
            .triggers
            .checked_add(counts.triggers)
            .expect("trigger count overflow");
        if advance {
            for word in tail.chunks_exact(2) {
                let word = u16::from_le_bytes([word[0], word[1]]);
                if word >> 12 == 8 {
                    self.process_time_high(word);
                }
            }
            self.recover_suffix_state(bytes);
        }
        total
    }

    fn recover_suffix_state(&mut self, bytes: &[u8]) {
        let (mut y, mut base, mut low, mut clock) = (false, false, false, false);
        let mut vector_advance = 0_u16;
        for word in bytes.chunks_exact(2).rev() {
            let word = u16::from_le_bytes([word[0], word[1]]);
            match word >> 12 {
                0 if !y => {
                    self.current_y = word & 0x07ff;
                    y = true;
                }
                3 if !base => {
                    self.current_base_x = (word & 0x07ff).wrapping_add(vector_advance);
                    self.current_polarity = ((word >> 11) & 1) as u8;
                    base = true;
                }
                4 if !base => vector_advance = vector_advance.wrapping_add(12),
                5 if !base => vector_advance = vector_advance.wrapping_add(8),
                6 => {
                    if !clock {
                        self.current_time = self.time_base + u64::from(word & 0x0fff);
                        clock = true;
                    }
                    if !low {
                        self.time_low = u64::from(word & 0x0fff);
                        low = true;
                    }
                }
                8 => clock = true,
                _ => {}
            }
            if y && base && low && clock {
                break;
            }
        }
        if !base {
            self.current_base_x = self.current_base_x.wrapping_add(vector_advance);
        }
    }

    fn prepare_parts<'a>(&self, mut bytes: &'a [u8], workers: usize) -> Option<Vec<Part<'a>>> {
        let mut state = self.snapshot();
        if !state.first_time_base_set {
            let offset = bytes.chunks_exact(2).position(|w| w[1] >> 4 == 8)? * 2;
            let high = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
            state.time_base = u64::from(high & 0x0fff) << 12;
            state.current_time = state.time_base;
            state.first_time_base_set = true;
            bytes = &bytes[offset + 2..];
        }
        if bytes.is_empty() {
            return None;
        }
        let chunk_bytes = (bytes.len() / 2).div_ceil(workers) * 2;
        let mut parts = Vec::with_capacity(workers);
        for (index, chunk) in bytes.chunks(chunk_bytes).enumerate() {
            let start = state.snapshot();
            let counts = state.count_and_advance(chunk, (index + 1) * chunk_bytes < bytes.len());
            parts.push(Part {
                bytes: chunk,
                decoder: start,
                counts,
            });
        }
        Some(parts)
    }

    fn parallel_plan<'a>(
        &mut self,
        bytes: &'a [u8],
    ) -> Option<(std::sync::Arc<rayon::ThreadPool>, Vec<Part<'a>>, ActiveCall)> {
        if bytes.len() < MIN_PARALLEL_BYTES || bytes.len() & 1 != 0 || self.pending_byte.is_some() {
            return None;
        }
        let limit = match self.parallelism {
            Parallelism::Off => return None,
            Parallelism::Auto => available_cpus().min(4),
            Parallelism::Threads(n) => n.min(available_cpus()),
        };
        if limit < 2 {
            return None;
        }
        let active = ACTIVE_CALLS.fetch_add(1, Ordering::Relaxed) + 1;
        let permit = ActiveCall;
        let workers = if self.parallelism == Parallelism::Auto {
            (available_cpus() / active).min(limit)
        } else {
            limit
        };
        if workers < 2 {
            return None;
        }
        // Each decoder owns its explicitly enabled pool. The caller supplies
        // one of the requested threads; snapshots share ownership, not new pools.
        if self.parallel_pool.is_none() {
            self.parallel_pool = rayon::ThreadPoolBuilder::new()
                .num_threads(limit - 1)
                .thread_name(|i| format!("evt3-decode-{i}"))
                .build()
                .ok()
                .map(std::sync::Arc::new);
        }
        let pool = self.parallel_pool.as_ref()?.clone();
        Some((pool, self.prepare_parts(bytes, workers)?, permit))
    }

    pub(crate) fn try_parallel_rows(
        &mut self,
        bytes: &[u8],
        cd: &mut Vec<CdEvent>,
        triggers: &mut Vec<TriggerEvent>,
    ) -> bool {
        let Some((pool, parts, _permit)) = self.parallel_plan(bytes) else {
            return false;
        };
        let cd_count: usize = parts
            .iter()
            .try_fold(0_usize, |total, p| total.checked_add(p.counts.cd))
            .expect("event count overflow");
        let trigger_count: usize = parts
            .iter()
            .try_fold(0_usize, |total, p| total.checked_add(p.counts.triggers))
            .expect("trigger count overflow");
        cd.reserve(cd_count);
        triggers.reserve(trigger_count);
        let mut cd_spare = cd.spare_capacity_mut();
        let mut trigger_spare = triggers.spare_capacity_mut();
        let mut jobs: Vec<_> = parts
            .into_iter()
            .map(|part| {
                let output = RowSlice {
                    cd: take_prefix(&mut cd_spare, part.counts.cd),
                    triggers: take_prefix(&mut trigger_spare, part.counts.triggers),
                    written: 0,
                    trigger_written: 0,
                };
                Job { part, output }
            })
            .collect();
        *self = run_jobs(&pool, &mut jobs);
        drop(jobs);
        // SAFETY: disjoint spare slices cover exactly these counted ranges.
        // Exact counts bound every CD write; trigger writes are bounds-checked.
        // Each worker asserts complete output.
        // A worker panic unwinds before lengths can expose uninitialized data.
        unsafe {
            cd.set_len(cd.len() + cd_count);
            triggers.set_len(triggers.len() + trigger_count);
        }
        true
    }

    pub(crate) fn try_parallel_columns(
        &mut self,
        bytes: &[u8],
        sink: &mut ColumnarEventSink,
    ) -> bool {
        let Some((pool, parts, _permit)) = self.parallel_plan(bytes) else {
            return false;
        };
        let cd_count: usize = parts
            .iter()
            .try_fold(0_usize, |total, p| total.checked_add(p.counts.cd))
            .expect("event count overflow");
        let trigger_count: usize = parts
            .iter()
            .try_fold(0_usize, |total, p| total.checked_add(p.counts.triggers))
            .expect("trigger count overflow");
        sink.reserve_cd(cd_count);
        sink.reserve_triggers(trigger_count);
        let mut x = sink.cd.x.spare_capacity_mut();
        let mut y = sink.cd.y.spare_capacity_mut();
        let mut p = sink.cd.polarity.spare_capacity_mut();
        let mut t = sink.cd.timestamp.spare_capacity_mut();
        let mut value = sink.triggers.value.spare_capacity_mut();
        let mut id = sink.triggers.id.spare_capacity_mut();
        let mut tt = sink.triggers.timestamp.spare_capacity_mut();
        let mut jobs: Vec<_> = parts
            .into_iter()
            .map(|part| {
                let output = ColumnSlice {
                    x: take_prefix(&mut x, part.counts.cd),
                    y: take_prefix(&mut y, part.counts.cd),
                    p: take_prefix(&mut p, part.counts.cd),
                    t: take_prefix(&mut t, part.counts.cd),
                    value: take_prefix(&mut value, part.counts.triggers),
                    id: take_prefix(&mut id, part.counts.triggers),
                    tt: take_prefix(&mut tt, part.counts.triggers),
                    written: 0,
                    trigger_written: 0,
                };
                Job { part, output }
            })
            .collect();
        *self = run_jobs(&pool, &mut jobs);
        drop(jobs);
        // SAFETY: as for rows, every disjoint range has been initialized and
        // checked before committing each column at its own original length.
        unsafe {
            sink.cd.x.set_len(sink.cd.x.len() + cd_count);
            sink.cd.y.set_len(sink.cd.y.len() + cd_count);
            sink.cd.polarity.set_len(sink.cd.polarity.len() + cd_count);
            sink.cd
                .timestamp
                .set_len(sink.cd.timestamp.len() + cd_count);
            sink.triggers
                .value
                .set_len(sink.triggers.value.len() + trigger_count);
            sink.triggers
                .id
                .set_len(sink.triggers.id.len() + trigger_count);
            sink.triggers
                .timestamp
                .set_len(sink.triggers.timestamp.len() + trigger_count);
        }
        true
    }
}

fn take_prefix<'a, T>(
    rest: &mut &'a mut [MaybeUninit<T>],
    count: usize,
) -> &'a mut [MaybeUninit<T>] {
    let (prefix, suffix) = std::mem::take(rest).split_at_mut(count);
    *rest = suffix;
    prefix
}

struct Job<'a, S> {
    part: Part<'a>,
    output: S,
}

fn run_jobs<S: EventSink + Send>(pool: &rayon::ThreadPool, jobs: &mut [Job<'_, S>]) -> Evt3Decoder {
    pool.in_place_scope(|scope| {
        let (caller_job, background) = jobs.split_last_mut().unwrap();
        let decode = |job: &mut Job<'_, S>| {
            job.part
                .decoder
                .decode_bytes_scalar(job.part.bytes, &mut job.output);
            assert_eq!(job.output.cd_len(), job.part.counts.cd);
            assert_eq!(job.output.trigger_len(), job.part.counts.triggers);
        };
        for job in background {
            scope.spawn(move |_| decode(job));
        }
        decode(caller_job);
    });
    jobs.last().unwrap().part.decoder.snapshot()
}

struct RowSlice<'a> {
    cd: &'a mut [MaybeUninit<CdEvent>],
    triggers: &'a mut [MaybeUninit<TriggerEvent>],
    written: usize,
    trigger_written: usize,
}

impl EventSink for RowSlice<'_> {
    fn cd_len(&self) -> usize {
        self.written
    }
    fn trigger_len(&self) -> usize {
        self.trigger_written
    }
    #[inline(always)]
    fn push_cd(&mut self, x: u16, y: u16, p: u8, t: u64) {
        debug_assert!(self.written < self.cd.len());
        // SAFETY: prepare_parts counts one output for ADDR_X and every set
        // validity bit for VECT_12/VECT_8 after initial synchronization. Each
        // part is even, has no pending byte, and starts synchronized. State
        // changes cannot suppress or add outputs, and fused vectors preserve
        // those populations. This write therefore stays in its disjoint,
        // exclusively borrowed slice of exactly part.counts.cd event slots.
        unsafe {
            let slot = self.cd.get_unchecked_mut(self.written);
            CdEvent::write_reserved(slot.as_mut_ptr(), x, y, p, t);
        }
        self.written += 1;
    }
    #[inline(always)]
    fn push_trigger(&mut self, value: u8, id: u8, t: u64) {
        self.triggers[self.trigger_written].write(TriggerEvent::new(value, id, t));
        self.trigger_written += 1;
    }
}

struct ColumnSlice<'a> {
    x: &'a mut [MaybeUninit<u16>],
    y: &'a mut [MaybeUninit<u16>],
    p: &'a mut [MaybeUninit<u8>],
    t: &'a mut [MaybeUninit<u64>],
    value: &'a mut [MaybeUninit<u8>],
    id: &'a mut [MaybeUninit<u8>],
    tt: &'a mut [MaybeUninit<u64>],
    written: usize,
    trigger_written: usize,
}

impl EventSink for ColumnSlice<'_> {
    fn cd_len(&self) -> usize {
        self.written
    }
    fn trigger_len(&self) -> usize {
        self.trigger_written
    }
    #[inline(always)]
    fn push_cd(&mut self, x: u16, y: u16, p: u8, t: u64) {
        debug_assert!(self.written < self.x.len());
        // SAFETY: the exact population argument in RowSlice also applies here.
        // Construction splits part.counts.cd slots from every CD column; the
        // slices remain equal in length, disjoint and exclusively borrowed.
        unsafe {
            self.x.get_unchecked_mut(self.written).write(x);
            self.y.get_unchecked_mut(self.written).write(y);
            self.p.get_unchecked_mut(self.written).write(p);
            self.t.get_unchecked_mut(self.written).write(t);
        }
        self.written += 1;
    }
    #[inline(always)]
    fn push_trigger(&mut self, value: u8, id: u8, t: u64) {
        self.value[self.trigger_written].write(value);
        self.id[self.trigger_written].write(id);
        self.tt[self.trigger_written].write(t);
        self.trigger_written += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mixed_stream() -> Vec<u8> {
        let mut words = vec![0x2001, 0x0007, 0x4fff, 0xa101, 0x8fff];
        let mut random = 42_u32;
        for i in 0..12_000_u16 {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            words.extend([
                0x6000 | (i & 0x0fff),
                i & 0x07ff,
                0x3000 | ((random as u16) & 0x0fff),
                0x4000 | (((random >> 12) as u16) & 0x0fff),
                0x4aaa,
                0x5f81,
                0x4001,
                0xa301,
                0x2807,
            ]);
            if i % 997 == 0 {
                words.extend([0x8fff, 0x8000]);
            }
        }
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn state(d: &Evt3Decoder) -> (u64, u64, u64, u64, bool, u16, u16, u8, Option<u8>) {
        (
            d.time_base,
            d.time_low,
            d.current_time,
            d.n_time_high_loops,
            d.first_time_base_set,
            d.current_y,
            d.current_base_x,
            d.current_polarity,
            d.pending_byte,
        )
    }

    #[test]
    fn discovered_chunk_states_match_sequential_decoding() {
        let bytes = mixed_stream();
        let mut initial = Evt3Decoder::new();
        // Skip pre-synchronization garbage in the same way as prepare_parts.
        let mut sink = ColumnarEventSink::default();
        initial.decode_bytes_scalar(&bytes[..10], &mut sink);
        for workers in [2, 3, 4, 17] {
            let parts = Evt3Decoder::new().prepare_parts(&bytes, workers).unwrap();
            let mut sequential = initial.snapshot();
            for part in parts {
                assert_eq!(state(&part.decoder), state(&sequential));
                sink.clear();
                sequential.decode_bytes_scalar(part.bytes, &mut sink);
                assert_eq!(part.counts.cd, sink.cd.len());
                assert_eq!(part.counts.triggers, sink.triggers.len());
            }
        }
    }

    #[test]
    fn parallel_outputs_and_next_packet_match_the_scalar_state_machine() {
        // Repeated recordings exercise long output ranges and timestamp wraps
        // before the final state is used for a following packet.
        let bytes = mixed_stream().repeat(6);
        let mut scalar = Evt3Decoder::new();
        let mut expected = ColumnarEventSink::default();
        // One word at a time also prevents the vector-group lookahead path.
        for word in bytes.chunks_exact(2) {
            scalar.decode_bytes_scalar(word, &mut expected);
        }
        let mut decoder = Evt3Decoder::new().with_parallelism(Parallelism::Threads(3));
        let mut actual = ColumnarEventSink::default();
        decoder.decode_bytes_columns(&bytes, &mut actual);
        assert_eq!(state(&decoder), state(&scalar));
        assert_eq!(actual.cd.x, expected.cd.x);
        assert_eq!(actual.cd.y, expected.cd.y);
        assert_eq!(actual.cd.polarity, expected.cd.polarity);
        assert_eq!(actual.cd.timestamp, expected.cd.timestamp);
        assert_eq!(actual.triggers.value, expected.triggers.value);
        assert_eq!(actual.triggers.id, expected.triggers.id);
        assert_eq!(actual.triggers.timestamp, expected.triggers.timestamp);
        let followup = [0x01, 0x40, 0x07, 0x20];
        decoder.decode_bytes_into(&followup, &mut actual);
        scalar.decode_bytes_scalar(&followup, &mut expected);
        assert_eq!(actual.cd.x, expected.cd.x);
        assert_eq!(actual.cd.timestamp, expected.cd.timestamp);
        assert_eq!(state(&decoder), state(&scalar));
    }

    #[test]
    fn concurrent_decoders_keep_independent_order_and_state() {
        let bytes = mixed_stream();
        let mut expected = ColumnarEventSink::default();
        Evt3Decoder::new().decode_bytes_scalar(&bytes, &mut expected);
        let start = std::sync::Barrier::new(4);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let (bytes, expected, start) = (&bytes, &expected, &start);
                scope.spawn(move || {
                    let mut actual = ColumnarEventSink::default();
                    start.wait();
                    Evt3Decoder::new()
                        .with_parallelism(Parallelism::Auto)
                        .decode_bytes_columns(bytes, &mut actual);
                    assert_eq!(actual.cd.x, expected.cd.x);
                    assert_eq!(actual.cd.y, expected.cd.y);
                    assert_eq!(actual.cd.polarity, expected.cd.polarity);
                    assert_eq!(actual.cd.timestamp, expected.cd.timestamp);
                    assert_eq!(actual.triggers.value, expected.triggers.value);
                    assert_eq!(actual.triggers.id, expected.triggers.id);
                    assert_eq!(actual.triggers.timestamp, expected.triggers.timestamp);
                });
            }
        });
    }
}
