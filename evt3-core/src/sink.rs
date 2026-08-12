//! Event sinks used by the decoder hot path.

use crate::types::{CdEvent, TriggerEvent};

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
