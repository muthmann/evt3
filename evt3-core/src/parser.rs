#![allow(clippy::unusual_byte_groupings)]
//! Low-level parsing of EVT 3.0 raw 16-bit words.
//!
//! This module provides functions to extract fields from each event type
//! using bitwise operations according to the EVT 3.0 specification.

use crate::types::RawEventType;

/// Extracts the 4-bit event type from a 16-bit word.
#[inline]
pub fn get_event_type(word: u16) -> u8 {
    ((word >> 12) & 0xF) as u8
}

/// Extracts the 12-bit payload from a 16-bit word.
#[inline]
pub fn get_payload(word: u16) -> u16 {
    word & 0x0FFF
}

// ============================================================================
// EVT_ADDR_Y (type = 0x0)
// Bits: [15:12] type | [11] system_type | [10:0] y
// ============================================================================

/// Extracts the Y coordinate from an EVT_ADDR_Y word.
#[inline]
pub fn addr_y_get_y(word: u16) -> u16 {
    word & 0x07FF // bits 10:0
}

/// Extracts the system type (master/slave) from an EVT_ADDR_Y word.
#[inline]
pub fn addr_y_get_system_type(word: u16) -> u8 {
    ((word >> 11) & 0x1) as u8
}

// ============================================================================
// EVT_ADDR_X (type = 0x2)
// Bits: [15:12] type | [11] polarity | [10:0] x
// ============================================================================

/// Extracts the X coordinate from an EVT_ADDR_X word.
#[inline]
pub fn addr_x_get_x(word: u16) -> u16 {
    word & 0x07FF // bits 10:0
}

/// Extracts the polarity from an EVT_ADDR_X word.
#[inline]
pub fn addr_x_get_polarity(word: u16) -> u8 {
    ((word >> 11) & 0x1) as u8
}

// ============================================================================
// VECT_BASE_X (type = 0x3)
// Bits: [15:12] type | [11] polarity | [10:0] x
// ============================================================================

/// Extracts the base X coordinate from a VECT_BASE_X word.
#[inline]
pub fn vect_base_x_get_x(word: u16) -> u16 {
    word & 0x07FF // bits 10:0
}

/// Extracts the polarity from a VECT_BASE_X word.
#[inline]
pub fn vect_base_x_get_polarity(word: u16) -> u8 {
    ((word >> 11) & 0x1) as u8
}

// ============================================================================
// VECT_12 (type = 0x4)
// Bits: [15:12] type | [11:0] valid (12-bit bitmask)
// ============================================================================

/// Extracts the 12-bit validity mask from a VECT_12 word.
#[inline]
pub fn vect_12_get_valid(word: u16) -> u16 {
    word & 0x0FFF // bits 11:0
}

// ============================================================================
// VECT_8 (type = 0x5)
// Bits: [15:12] type | [11:8] unused | [7:0] valid (8-bit bitmask)
// ============================================================================

/// Extracts the 8-bit validity mask from a VECT_8 word.
#[inline]
pub fn vect_8_get_valid(word: u16) -> u8 {
    (word & 0x00FF) as u8 // bits 7:0
}

// ============================================================================
// EVT_TIME_LOW (type = 0x6) / EVT_TIME_HIGH (type = 0x8)
// Bits: [15:12] type | [11:0] time
// ============================================================================

/// Extracts the 12-bit time value from a TIME_LOW or TIME_HIGH word.
#[inline]
pub fn time_get_value(word: u16) -> u16 {
    word & 0x0FFF // bits 11:0
}

// ============================================================================
// EXT_TRIGGER (type = 0xA)
// Bits: [15:12] type | [11:8] id | [7:1] unused | [0] value
// ============================================================================

/// Extracts the trigger channel ID from an EXT_TRIGGER word.
#[inline]
pub fn ext_trigger_get_id(word: u16) -> u8 {
    ((word >> 8) & 0x0F) as u8 // bits 11:8
}

/// Extracts the trigger value (edge polarity) from an EXT_TRIGGER word.
#[inline]
pub fn ext_trigger_get_value(word: u16) -> u8 {
    (word & 0x01) as u8 // bit 0
}

/// Parses the event type from a 16-bit word.
#[inline]
pub fn parse_event_type(word: u16) -> Option<RawEventType> {
    RawEventType::from_u8(get_event_type(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addr_y_parsing() {
        // type=0, system_type=0, y=500
        let word: u16 = 0b0000_0_00111110100;
        assert_eq!(addr_y_get_y(word), 500);
        assert_eq!(addr_y_get_system_type(word), 0);

        // type=0, system_type=1, y=100
        let word2: u16 = 0b0000_1_00001100100;
        assert_eq!(addr_y_get_y(word2), 100);
        assert_eq!(addr_y_get_system_type(word2), 1);
    }

    #[test]
    fn test_addr_x_parsing() {
        // type=2, pol=1, x=300
        let word: u16 = 0b0010_1_00100101100;
        assert_eq!(get_event_type(word), 0x2);
        assert_eq!(addr_x_get_x(word), 300);
        assert_eq!(addr_x_get_polarity(word), 1);
    }

    #[test]
    fn test_vect_12_parsing() {
        // type=4, valid=0b101010101010
        let word: u16 = 0b0100_101010101010;
        assert_eq!(get_event_type(word), 0x4);
        assert_eq!(vect_12_get_valid(word), 0b101010101010);
    }

    #[test]
    fn test_time_parsing() {
        // TIME_HIGH type=8, time=0xABC
        let word: u16 = 0b1000_101010111100;
        assert_eq!(get_event_type(word), 0x8);
        assert_eq!(time_get_value(word), 0xABC);
    }

    #[test]
    fn test_ext_trigger_parsing() {
        // type=A, id=2, value=1
        let word: u16 = 0b1010_0010_0000000_1;
        assert_eq!(get_event_type(word), 0xA);
        assert_eq!(ext_trigger_get_id(word), 2);
        assert_eq!(ext_trigger_get_value(word), 1);
    }
}
