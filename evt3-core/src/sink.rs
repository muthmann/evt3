//! Event sinks used by the decoder hot path.

use crate::types::{CdEvent, TriggerEvent};
use crate::Evt3Decoder;

// Keep common 8-KiB packets in one reservation and state-update batch.
const OUTPUT_BLOCK_WORDS: usize = 4096;

fn word_output_counts(words: &[u16]) -> (usize, usize) {
    words.iter().fold((0, 0), |(cd, triggers), &word| {
        let events = match word >> 12 {
            2 => 1,
            4 => (word & 0x0fff).count_ones() as usize,
            5 => (word & 0x00ff).count_ones() as usize,
            _ => 0,
        };
        (cd + events, triggers + usize::from(word >> 12 == 10))
    })
}

fn spare<T>(v: &Vec<T>) -> usize {
    v.capacity() - v.len()
}

/// Receives decoded events without prescribing their storage layout.
///
/// Implementations can collect row-oriented Rust events, build columnar
/// arrays for NumPy, or process events incrementally. Sink methods are
/// intentionally infallible so the compiler can keep the per-word decode loop
/// small and easy to inline.
pub trait EventSink {
    /// Reserves storage for additional CD events when useful.
    fn reserve_cd(&mut self, _additional: usize) {}

    /// Reserves storage for additional trigger events when useful.
    fn reserve_triggers(&mut self, _additional: usize) {}

    /// Returns the number of CD events already written to this sink.
    fn cd_len(&self) -> usize;

    /// Returns the number of trigger events already written to this sink.
    fn trigger_len(&self) -> usize;

    /// Receives one decoded CD event.
    fn push_cd(&mut self, x: u16, y: u16, polarity: u8, timestamp: u64);

    /// Receives one decoded external trigger event.
    fn push_trigger(&mut self, value: u8, id: u8, timestamp: u64);
}

/// Decoded CD events stored in a structure-of-arrays layout.
///
/// The field widths match the public NumPy representation and use 13 bytes of
/// payload per event rather than the 16-byte aligned [`CdEvent`] layout.
#[derive(Debug, Default)]
pub struct EventColumns {
    pub x: Vec<u16>,
    pub y: Vec<u16>,
    pub polarity: Vec<u8>,
    pub timestamp: Vec<u64>,
}

impl EventColumns {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            x: Vec::with_capacity(capacity),
            y: Vec::with_capacity(capacity),
            polarity: Vec::with_capacity(capacity),
            timestamp: Vec::with_capacity(capacity),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.x.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    pub fn clear(&mut self) {
        self.x.clear();
        self.y.clear();
        self.polarity.clear();
        self.timestamp.clear();
    }
}

/// Decoded trigger events stored in a structure-of-arrays layout.
#[derive(Debug, Default)]
pub struct TriggerColumns {
    pub value: Vec<u8>,
    pub id: Vec<u8>,
    pub timestamp: Vec<u64>,
}

impl TriggerColumns {
    #[inline]
    pub fn len(&self) -> usize {
        self.value.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.id.clear();
        self.timestamp.clear();
    }
}

/// A reusable columnar sink for CD and trigger events.
#[derive(Debug, Default)]
pub struct ColumnarEventSink {
    pub cd: EventColumns,
    pub triggers: TriggerColumns,
}

impl ColumnarEventSink {
    fn use_bulk_budget(&self, cd: usize) -> bool {
        // A cleared allocation is being reused. Grow it once to the bounded
        // worst case, while fresh Python outputs reserve only counted events.
        (self.cd.is_empty() && self.cd.x.capacity() > 0)
            || (spare(&self.cd.x) >= cd
                && spare(&self.cd.y) >= cd
                && spare(&self.cd.polarity) >= cd
                && spare(&self.cd.timestamp) >= cd)
    }

    pub fn with_cd_capacity(capacity: usize) -> Self {
        Self {
            cd: EventColumns::with_capacity(capacity),
            triggers: TriggerColumns::default(),
        }
    }

    pub fn clear(&mut self) {
        self.cd.clear();
        self.triggers.clear();
    }
}

impl ColumnarEventSink {
    pub(crate) fn decode_byte_chunk(&mut self, decoder: &mut Evt3Decoder, bytes: &[u8]) {
        for block in bytes.chunks(2 * OUTPUT_BLOCK_WORDS) {
            let words = block.len().div_ceil(2);
            let budget = if self.use_bulk_budget(12 * words) {
                (12 * words, words)
            } else {
                decoder.output_counts(block)
            };
            let mut writer = ColumnarBatchWriter::new(self, budget);
            decoder.decode_bytes_scalar(block, &mut writer);
        }
    }
}

impl EventSink for ColumnarEventSink {
    fn reserve_cd(&mut self, additional: usize) {
        self.cd.x.reserve(additional);
        self.cd.y.reserve(additional);
        self.cd.polarity.reserve(additional);
        self.cd.timestamp.reserve(additional);
    }

    fn reserve_triggers(&mut self, additional: usize) {
        self.triggers.value.reserve(additional);
        self.triggers.id.reserve(additional);
        self.triggers.timestamp.reserve(additional);
    }

    #[inline(always)]
    fn cd_len(&self) -> usize {
        self.cd.len()
    }

    #[inline(always)]
    fn trigger_len(&self) -> usize {
        self.triggers.len()
    }

    #[inline(always)]
    fn push_cd(&mut self, x: u16, y: u16, polarity: u8, timestamp: u64) {
        self.cd.x.push(x);
        self.cd.y.push(y);
        self.cd.polarity.push(polarity);
        self.cd.timestamp.push(timestamp);
    }

    #[inline(always)]
    fn push_trigger(&mut self, value: u8, id: u8, timestamp: u64) {
        self.triggers.value.push(value);
        self.triggers.id.push(id);
        self.triggers.timestamp.push(timestamp);
    }
}

pub(crate) struct VecEventSink<'a> {
    pub cd: &'a mut Vec<CdEvent>,
    pub triggers: &'a mut Vec<TriggerEvent>,
}

impl VecEventSink<'_> {
    pub(crate) fn decode_word_chunk(&mut self, decoder: &mut Evt3Decoder, words: &[u16]) {
        for block in words.chunks(OUTPUT_BLOCK_WORDS) {
            let budget = if spare(self.cd) >= 12 * block.len()
                || (self.cd.is_empty() && self.cd.capacity() > 0)
            {
                (12 * block.len(), block.len())
            } else {
                word_output_counts(block)
            };
            let mut writer = RowBatchWriter::new(self, budget);
            decoder.decode_buffer_scalar(block, &mut writer);
        }
    }

    pub(crate) fn decode_byte_chunk(&mut self, decoder: &mut Evt3Decoder, bytes: &[u8]) {
        for block in bytes.chunks(2 * OUTPUT_BLOCK_WORDS) {
            let words = block.len().div_ceil(2);
            let budget =
                if spare(self.cd) >= 12 * words || (self.cd.is_empty() && self.cd.capacity() > 0) {
                    (12 * words, words)
                } else {
                    decoder.output_counts(block)
                };
            let mut writer = RowBatchWriter::new(self, budget);
            decoder.decode_bytes_scalar(block, &mut writer);
        }
    }
}

impl EventSink for VecEventSink<'_> {
    fn reserve_cd(&mut self, additional: usize) {
        self.cd.reserve(additional);
    }

    fn reserve_triggers(&mut self, additional: usize) {
        self.triggers.reserve(additional);
    }

    #[inline(always)]
    fn cd_len(&self) -> usize {
        self.cd.len()
    }

    #[inline(always)]
    fn trigger_len(&self) -> usize {
        self.triggers.len()
    }

    #[inline(always)]
    fn push_cd(&mut self, x: u16, y: u16, polarity: u8, timestamp: u64) {
        self.cd.push(CdEvent::new(x, y, polarity, timestamp));
    }

    #[inline(always)]
    fn push_trigger(&mut self, value: u8, id: u8, timestamp: u64) {
        self.triggers.push(TriggerEvent::new(value, id, timestamp));
    }
}

// These private writers are used only for a bounded block of decoder input.
// One input word emits at most 12 CD events or one trigger. An odd byte block
// can complete at most ceil(bytes / 2) words, including the carried byte.
// Callers reserve either that maximum or the sum of event mask populations
// and trigger tags, including a carried byte. Ignoring pre-sync words can only
// reduce the number emitted. These budgets cover each complete output block.
// The vectors stay exclusively borrowed and cannot reallocate until Drop has
// committed only the initialized prefix, including during unwinding.
struct RowBatchWriter<'a, 'b> {
    sink: &'a mut VecEventSink<'b>,
    cd: *mut CdEvent,
    cd_start: *mut CdEvent,
    triggers: *mut TriggerEvent,
    trigger_written: usize,
    budget: (usize, usize),
}

impl<'a, 'b> RowBatchWriter<'a, 'b> {
    fn new(sink: &'a mut VecEventSink<'b>, budget: (usize, usize)) -> Self {
        sink.reserve_cd(budget.0);
        sink.reserve_triggers(budget.1);
        // SAFETY: len <= capacity; add(len) is the start of reserved spare storage.
        let cd = unsafe { sink.cd.as_mut_ptr().add(sink.cd.len()) };
        // SAFETY: same bound for the separately reserved trigger vector.
        let triggers = unsafe { sink.triggers.as_mut_ptr().add(sink.triggers.len()) };
        Self {
            sink,
            cd,
            cd_start: cd,
            triggers,
            trigger_written: 0,
            budget,
        }
    }

    fn cd_written(&self) -> usize {
        // SAFETY: both pointers originate at the same reserved vector slot.
        // Each successful write advances cd once, at most to the allocation's
        // one-past end. The exclusively borrowed vector cannot reallocate.
        unsafe { self.cd.offset_from(self.cd_start) as usize }
    }
}

impl EventSink for RowBatchWriter<'_, '_> {
    fn cd_len(&self) -> usize {
        self.sink.cd.len() + self.cd_written()
    }
    fn trigger_len(&self) -> usize {
        self.sink.triggers.len() + self.trigger_written
    }

    #[inline(always)]
    fn push_cd(&mut self, x: u16, y: u16, polarity: u8, timestamp: u64) {
        debug_assert!(self.cd_written() < self.budget.0);
        // SAFETY: the input-derived budget bounds the emitted events and the
        // reserved allocation remains exclusively borrowed by this writer.
        unsafe {
            CdEvent::write_reserved(self.cd, x, y, polarity, timestamp);
            self.cd = self.cd.add(1);
        }
    }

    #[inline(always)]
    fn push_trigger(&mut self, value: u8, id: u8, timestamp: u64) {
        debug_assert!(self.trigger_written < self.budget.1);
        // SAFETY: the input-derived trigger budget covers this reserved slot.
        unsafe {
            self.triggers
                .add(self.trigger_written)
                .write(TriggerEvent::new(value, id, timestamp));
        }
        self.trigger_written += 1;
    }
}

impl Drop for RowBatchWriter<'_, '_> {
    fn drop(&mut self) {
        let cd_written = self.cd_written();
        // SAFETY: each count advances only after a complete initialized event
        // was written. The reserved capacity covers these appended prefixes.
        unsafe {
            self.sink.cd.set_len(self.sink.cd.len() + cd_written);
            self.sink
                .triggers
                .set_len(self.sink.triggers.len() + self.trigger_written);
        }
    }
}

struct ColumnarBatchWriter<'a> {
    sink: &'a mut ColumnarEventSink,
    x: *mut u16,
    y: *mut u16,
    p: *mut u8,
    t: *mut u64,
    trigger_value: *mut u8,
    trigger_id: *mut u8,
    trigger_time: *mut u64,
    cd_written: usize,
    trigger_written: usize,
    budget: (usize, usize),
}

impl<'a> ColumnarBatchWriter<'a> {
    fn new(sink: &'a mut ColumnarEventSink, budget: (usize, usize)) -> Self {
        sink.reserve_cd(budget.0);
        sink.reserve_triggers(budget.1);
        // SAFETY: every pointer starts at its own vector's length, within its
        // reserved allocation. This also handles unequal public column lengths.
        unsafe {
            Self {
                x: sink.cd.x.as_mut_ptr().add(sink.cd.x.len()),
                y: sink.cd.y.as_mut_ptr().add(sink.cd.y.len()),
                p: sink.cd.polarity.as_mut_ptr().add(sink.cd.polarity.len()),
                t: sink.cd.timestamp.as_mut_ptr().add(sink.cd.timestamp.len()),
                trigger_value: sink
                    .triggers
                    .value
                    .as_mut_ptr()
                    .add(sink.triggers.value.len()),
                trigger_id: sink.triggers.id.as_mut_ptr().add(sink.triggers.id.len()),
                trigger_time: sink
                    .triggers
                    .timestamp
                    .as_mut_ptr()
                    .add(sink.triggers.timestamp.len()),
                sink,
                cd_written: 0,
                trigger_written: 0,
                budget,
            }
        }
    }
}

impl EventSink for ColumnarBatchWriter<'_> {
    fn cd_len(&self) -> usize {
        self.sink.cd.len() + self.cd_written
    }
    fn trigger_len(&self) -> usize {
        self.sink.triggers.len() + self.trigger_written
    }

    #[inline(always)]
    fn push_cd(&mut self, x: u16, y: u16, polarity: u8, timestamp: u64) {
        debug_assert!(self.cd_written < self.budget.0);
        // SAFETY: all columns reserve the input-derived event budget. Each count
        // advances only after writing the complete event to every column.
        unsafe {
            self.x.add(self.cd_written).write(x);
            self.y.add(self.cd_written).write(y);
            self.p.add(self.cd_written).write(polarity);
            self.t.add(self.cd_written).write(timestamp);
        }
        self.cd_written += 1;
    }

    #[inline(always)]
    fn push_trigger(&mut self, value: u8, id: u8, timestamp: u64) {
        debug_assert!(self.trigger_written < self.budget.1);
        // SAFETY: each trigger column reserves the input-derived trigger budget.
        unsafe {
            self.trigger_value.add(self.trigger_written).write(value);
            self.trigger_id.add(self.trigger_written).write(id);
            self.trigger_time.add(self.trigger_written).write(timestamp);
        }
        self.trigger_written += 1;
    }
}

impl Drop for ColumnarBatchWriter<'_> {
    fn drop(&mut self) {
        // SAFETY: only the initialized prefixes are appended to each original
        // column length. Vectors have not reallocated since construction.
        unsafe {
            self.sink
                .cd
                .x
                .set_len(self.sink.cd.x.len() + self.cd_written);
            self.sink
                .cd
                .y
                .set_len(self.sink.cd.y.len() + self.cd_written);
            self.sink
                .cd
                .polarity
                .set_len(self.sink.cd.polarity.len() + self.cd_written);
            self.sink
                .cd
                .timestamp
                .set_len(self.sink.cd.timestamp.len() + self.cd_written);
            self.sink
                .triggers
                .value
                .set_len(self.sink.triggers.value.len() + self.trigger_written);
            self.sink
                .triggers
                .id
                .set_len(self.sink.triggers.id.len() + self.trigger_written);
            self.sink
                .triggers
                .timestamp
                .set_len(self.sink.triggers.timestamp.len() + self.trigger_written);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deliberately uses the safe, per-event Vec::push fallback instead of either
    // bounded writer, to check storage and chunk accounting independently.
    #[derive(Default)]
    struct SafeRows {
        cd: Vec<CdEvent>,
        triggers: Vec<TriggerEvent>,
    }

    impl EventSink for SafeRows {
        fn cd_len(&self) -> usize {
            self.cd.len()
        }
        fn trigger_len(&self) -> usize {
            self.triggers.len()
        }
        fn push_cd(&mut self, x: u16, y: u16, p: u8, t: u64) {
            self.cd.push(CdEvent::new(x, y, p, t));
        }
        fn push_trigger(&mut self, value: u8, id: u8, t: u64) {
            self.triggers.push(TriggerEvent::new(value, id, t));
        }
    }

    #[test]
    fn bounded_writers_match_safe_sink_at_maximum_density_and_odd_boundaries() {
        let mut words: Vec<u16> = vec![0x8000, 0x6fff, 0x07ff];
        for round in 0..9 {
            words.push(0x3800 | round);
            // Every VECT_12 word emits the maximum twelve events. The base is
            // reset between runs so even debug builds stay within u16 range.
            words.extend(std::iter::repeat_n(0x4fff, 2049));
            words.extend(std::iter::repeat_n(0xa701, 2049));
        }
        let bytes: Vec<_> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        let mut expected = SafeRows::default();
        Evt3Decoder::new().decode_bytes_into(&bytes, &mut expected);
        assert_eq!(expected.cd.len(), 9 * 2049 * 12);
        assert_eq!(expected.triggers.len(), 9 * 2049);

        for size in [1, 3, 2047, 2048, 2049, 8191, 8192, 8193, bytes.len()] {
            let mut decoder = Evt3Decoder::new();
            let mut cd = vec![CdEvent::new(42, 43, 1, 44)];
            let mut triggers = vec![TriggerEvent::new(0, 2, 45)];
            let mut columns = ColumnarEventSink::default();
            let mut column_decoder = Evt3Decoder::new();
            for chunk in bytes.chunks(size) {
                decoder.decode_bytes(chunk, &mut cd, &mut triggers).unwrap();
                column_decoder.decode_bytes_columns(chunk, &mut columns);
            }
            decoder.finish_stream().unwrap();
            column_decoder.finish_stream().unwrap();
            assert_eq!(&cd[1..], expected.cd);
            assert_eq!(&triggers[1..], expected.triggers);
            assert_eq!(
                columns.cd.x,
                expected.cd.iter().map(|e| e.x).collect::<Vec<_>>()
            );
            assert_eq!(
                columns.cd.y,
                expected.cd.iter().map(|e| e.y).collect::<Vec<_>>()
            );
            assert_eq!(
                columns.cd.polarity,
                expected.cd.iter().map(|e| e.polarity).collect::<Vec<_>>()
            );
            assert_eq!(
                columns.cd.timestamp,
                expected.cd.iter().map(|e| e.timestamp).collect::<Vec<_>>()
            );
            assert_eq!(
                columns.triggers.value,
                expected
                    .triggers
                    .iter()
                    .map(|e| e.value)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                columns.triggers.id,
                expected.triggers.iter().map(|e| e.id).collect::<Vec<_>>()
            );
            assert_eq!(
                columns.triggers.timestamp,
                expected
                    .triggers
                    .iter()
                    .map(|e| e.timestamp)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn unequal_public_columns_append_at_their_own_lengths() {
        let mut sink = ColumnarEventSink::default();
        sink.cd.y.extend([100, 200]);
        sink.cd.timestamp.push(300);
        sink.triggers.id.extend([4, 5, 6]);
        let bytes: Vec<_> = [0x8001_u16, 0x002a, 0x2807, 0xa301]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        Evt3Decoder::new().decode_bytes_columns(&bytes, &mut sink);
        assert_eq!(sink.cd.x, [7]);
        assert_eq!(sink.cd.y, [100, 200, 42]);
        assert_eq!(sink.cd.polarity, [1]);
        assert_eq!(sink.cd.timestamp, [300, 4096]);
        assert_eq!(sink.triggers.value, [1]);
        assert_eq!(sink.triggers.id, [4, 5, 6, 3]);
        assert_eq!(sink.triggers.timestamp, [4096]);
    }

    #[test]
    fn unwinding_commits_only_initialized_events() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let event = CdEvent::new(65535, 65534, 1, u64::MAX);
        let mut cd = vec![event];
        let mut triggers = Vec::new();
        let mut sink = VecEventSink {
            cd: &mut cd,
            triggers: &mut triggers,
        };
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let mut writer = RowBatchWriter::new(&mut sink, (24, 2));
            writer.push_cd(event.x, event.y, event.polarity, event.timestamp);
            writer.push_trigger(1, 15, u64::MAX);
            panic!("interrupt output block");
        }))
        .is_err());
        assert_eq!(cd, [event, event]);
        assert_eq!(triggers, [TriggerEvent::new(1, 15, u64::MAX)]);

        let mut columns = ColumnarEventSink::default();
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let mut writer = ColumnarBatchWriter::new(&mut columns, (24, 2));
            writer.push_cd(event.x, event.y, event.polarity, event.timestamp);
            writer.push_trigger(1, 15, u64::MAX);
            panic!("interrupt column output block");
        }))
        .is_err());
        assert_eq!(columns.cd.x, [event.x]);
        assert_eq!(columns.cd.y, [event.y]);
        assert_eq!(columns.cd.polarity, [event.polarity]);
        assert_eq!(columns.cd.timestamp, [event.timestamp]);
        assert_eq!(columns.triggers.timestamp, [u64::MAX]);
    }
}
