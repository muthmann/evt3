//! Core types for EVT 3.0 event data.
//!
//! This module defines the event structures and raw event types according to
//! the Prophesee EVT 3.0 specification.

/// A decoded Change Detection (CD) event.
///
/// CD events represent brightness changes detected by the event camera sensor.
/// Each event contains the pixel coordinates, polarity (increase/decrease in
/// brightness), and timestamp in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct CdEvent {
    /// X coordinate of the pixel (0-2047)
    pub x: u16,
    /// Y coordinate of the pixel (0-2047)
    pub y: u16,
    /// Event polarity: 0 = OFF (decrease), 1 = ON (increase in brightness)
    pub polarity: u8,
    /// Timestamp in microseconds
    pub timestamp: u64,
}

impl CdEvent {
    /// Creates a new CD event.
    #[inline]
    pub fn new(x: u16, y: u16, polarity: u8, timestamp: u64) -> Self {
        Self {
            x,
            y,
            polarity,
            timestamp,
        }
    }
}

/// An external trigger event.
///
/// Trigger events indicate that an edge (change of electrical state) was
/// detected on an external trigger signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct TriggerEvent {
    /// Trigger value (edge polarity): 0 = falling edge, 1 = rising edge
    pub value: u8,
    /// Trigger channel ID
    pub id: u8,
    /// Timestamp in microseconds
    pub timestamp: u64,
}

impl TriggerEvent {
    /// Creates a new trigger event.
    #[inline]
    pub fn new(value: u8, id: u8, timestamp: u64) -> Self {
        Self {
            value,
            id,
            timestamp,
        }
    }
}

/// EVT 3.0 raw event types.
///
/// Each 16-bit word in the EVT 3.0 format has a 4-bit type field in the MSB
/// that identifies the event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RawEventType {
    /// Y coordinate and system type (0x0)
    AddrY = 0x0,
    /// Single valid event with X coordinate and polarity (0x2)
    AddrX = 0x2,
    /// Base X coordinate for subsequent vector events (0x3)
    VectBaseX = 0x3,
    /// Vector event with 12 validity bits (0x4)
    Vect12 = 0x4,
    /// Vector event with 8 validity bits (0x5)
    Vect8 = 0x5,
    /// Lower 12 bits of timestamp (0x6)
    TimeLow = 0x6,
    /// Continued event with 4 bits of data (0x7)
    Continued4 = 0x7,
    /// Upper 12 bits of timestamp (0x8)
    TimeHigh = 0x8,
    /// External trigger event (0xA)
    ExtTrigger = 0xA,
    /// Extension event type (0xE)
    Others = 0xE,
    /// Continued event with 12 bits of data (0xF)
    Continued12 = 0xF,
}

impl RawEventType {
    /// Attempts to parse an event type from a 4-bit value.
    #[inline]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x0 => Some(Self::AddrY),
            0x2 => Some(Self::AddrX),
            0x3 => Some(Self::VectBaseX),
            0x4 => Some(Self::Vect12),
            0x5 => Some(Self::Vect8),
            0x6 => Some(Self::TimeLow),
            0x7 => Some(Self::Continued4),
            0x8 => Some(Self::TimeHigh),
            0xA => Some(Self::ExtTrigger),
            0xE => Some(Self::Others),
            0xF => Some(Self::Continued12),
            _ => None,
        }
    }
}

/// Sensor metadata parsed from file headers.
#[derive(Debug, Clone)]
pub struct SensorMetadata {
    /// Sensor width in pixels
    pub width: u32,
    /// Sensor height in pixels
    pub height: u32,
}

impl Default for SensorMetadata {
    fn default() -> Self {
        // Default to Gen4 sensor geometry (1280x720)
        Self {
            width: 1280,
            height: 720,
        }
    }
}

/// Result of decoding an EVT 3.0 file.
#[derive(Debug)]
pub struct DecodeResult {
    /// Decoded CD events
    pub cd_events: Vec<CdEvent>,
    /// Decoded trigger events
    pub trigger_events: Vec<TriggerEvent>,
    /// Sensor metadata
    pub metadata: SensorMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_event_type_parsing() {
        assert_eq!(RawEventType::from_u8(0x0), Some(RawEventType::AddrY));
        assert_eq!(RawEventType::from_u8(0x2), Some(RawEventType::AddrX));
        assert_eq!(RawEventType::from_u8(0x8), Some(RawEventType::TimeHigh));
        assert_eq!(RawEventType::from_u8(0x1), None); // Reserved
        assert_eq!(RawEventType::from_u8(0x9), None); // Reserved
    }

    #[test]
    fn test_cd_event_creation() {
        let event = CdEvent::new(100, 200, 1, 12345);
        assert_eq!(event.x, 100);
        assert_eq!(event.y, 200);
        assert_eq!(event.polarity, 1);
        assert_eq!(event.timestamp, 12345);
    }
}
