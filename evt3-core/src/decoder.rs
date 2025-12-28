//! Stateful EVT 3.0 decoder.
//!
//! This module implements the EVT 3.0 decoding state machine that tracks
//! timestamp, coordinates, and polarity across events.

use crate::parser;
use crate::types::{CdEvent, DecodeResult, RawEventType, SensorMetadata, TriggerEvent};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during EVT 3.0 decoding.
#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid file format: {0}")]
    InvalidFormat(String),

    #[error("Unexpected end of file")]
    UnexpectedEof,
}

/// Constants for timestamp handling (matching C++ reference).
const MAX_TIMESTAMP_BASE: u64 = ((1u64 << 12) - 1) << 12; // 16773120us
const TIME_LOOP: u64 = MAX_TIMESTAMP_BASE + (1 << 12); // 16777216us
const LOOP_THRESHOLD: u64 = 10 << 12; // Threshold for loop detection

/// Buffer size for reading raw data (number of 16-bit words).
const READ_BUFFER_SIZE: usize = 1_000_000;

/// Stateful EVT 3.0 decoder.
///
/// Maintains internal state to properly reconstruct the event stream according
/// to the EVT 3.0 specification.
#[derive(Debug)]
pub struct Evt3Decoder {
    // Timestamp state
    time_base: u64,
    time_low: u64,
    current_time: u64,
    n_time_high_loops: u64,
    first_time_base_set: bool,

    // Address/polarity state
    current_y: u16,
    current_base_x: u16,
    current_polarity: u8,

    // Metadata
    pub metadata: SensorMetadata,
}

impl Default for Evt3Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Evt3Decoder {
    /// Creates a new decoder with default state.
    pub fn new() -> Self {
        Self {
            time_base: 0,
            time_low: 0,
            current_time: 0,
            n_time_high_loops: 0,
            first_time_base_set: false,
            current_y: 0,
            current_base_x: 0,
            current_polarity: 0,
            metadata: SensorMetadata::default(),
        }
    }

    /// Resets the decoder state.
    pub fn reset(&mut self) {
        self.time_base = 0;
        self.time_low = 0;
        self.current_time = 0;
        self.n_time_high_loops = 0;
        self.first_time_base_set = false;
        self.current_y = 0;
        self.current_base_x = 0;
        self.current_polarity = 0;
    }

    /// Decodes a buffer of 16-bit words into CD and trigger events.
    ///
    /// This is the core decoding function that processes raw EVT 3.0 data.
    pub fn decode_buffer(
        &mut self,
        words: &[u16],
        cd_events: &mut Vec<CdEvent>,
        trigger_events: &mut Vec<TriggerEvent>,
    ) {
        let mut iter = words.iter();

        // Skip until first TIME_HIGH if not yet set
        if !self.first_time_base_set {
            for &word in iter.by_ref() {
                let event_type = parser::get_event_type(word);
                if event_type == RawEventType::TimeHigh as u8 {
                    let time_val = parser::time_get_value(word);
                    self.time_base = (time_val as u64) << 12;
                    self.current_time = self.time_base;
                    self.first_time_base_set = true;
                    break;
                }
            }
        }

        // Process remaining events
        for &word in iter {
            let event_type = parser::get_event_type(word);

            match RawEventType::from_u8(event_type) {
                Some(RawEventType::AddrX) => {
                    let x = parser::addr_x_get_x(word);
                    let pol = parser::addr_x_get_polarity(word);
                    cd_events.push(CdEvent::new(x, self.current_y, pol, self.current_time));
                }

                Some(RawEventType::Vect12) => {
                    let valid = parser::vect_12_get_valid(word);
                    self.process_vector_events(valid as u32, 12, cd_events);
                }

                Some(RawEventType::Vect8) => {
                    let valid = parser::vect_8_get_valid(word);
                    self.process_vector_events(valid as u32, 8, cd_events);
                }

                Some(RawEventType::AddrY) => {
                    self.current_y = parser::addr_y_get_y(word);
                }

                Some(RawEventType::VectBaseX) => {
                    self.current_base_x = parser::vect_base_x_get_x(word);
                    self.current_polarity = parser::vect_base_x_get_polarity(word);
                }

                Some(RawEventType::TimeHigh) => {
                    self.process_time_high(word);
                }

                Some(RawEventType::TimeLow) => {
                    self.time_low = parser::time_get_value(word) as u64;
                    self.current_time = self.time_base + self.time_low;
                }

                Some(RawEventType::ExtTrigger) => {
                    let value = parser::ext_trigger_get_value(word);
                    let id = parser::ext_trigger_get_id(word);
                    trigger_events.push(TriggerEvent::new(value, id, self.current_time));
                }

                Some(RawEventType::Continued4)
                | Some(RawEventType::Others)
                | Some(RawEventType::Continued12) => {
                    // These event types are not commonly used for CD events
                    // and are skipped in this implementation
                }

                None => {
                    // Reserved/unknown event type, skip
                }
            }
        }
    }

    /// Processes TIME_HIGH events with loop detection.
    #[inline]
    fn process_time_high(&mut self, word: u16) {
        let time_val = parser::time_get_value(word);
        let mut new_time_base = ((time_val as u64) << 12) + (self.n_time_high_loops * TIME_LOOP);

        // Detect time high loop (went back in time due to wrap)
        if self.time_base > new_time_base
            && (self.time_base - new_time_base) >= (MAX_TIMESTAMP_BASE - LOOP_THRESHOLD)
        {
            new_time_base += TIME_LOOP;
            self.n_time_high_loops += 1;
        }

        self.time_base = new_time_base;
        self.current_time = self.time_base;
    }

    /// Processes vector events (VECT_12 or VECT_8) and emits CD events.
    #[inline]
    fn process_vector_events(&mut self, mut valid: u32, count: u16, cd_events: &mut Vec<CdEvent>) {
        let end_x = self.current_base_x + count;

        for x in self.current_base_x..end_x {
            if valid & 0x1 != 0 {
                cd_events.push(CdEvent::new(
                    x,
                    self.current_y,
                    self.current_polarity,
                    self.current_time,
                ));
            }
            valid >>= 1;
        }

        self.current_base_x = end_x;
    }

    /// Decodes an EVT 3.0 file from disk.
    ///
    /// Parses the file header (if present) and decodes all events.
    pub fn decode_file<P: AsRef<Path>>(&mut self, path: P) -> Result<DecodeResult, DecodeError> {
        let file = File::open(path.as_ref())?;
        let mut reader = BufReader::new(file);

        // Parse header
        self.parse_header(&mut reader)?;

        // Read and decode raw data
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();
        let mut buffer = vec![0u8; READ_BUFFER_SIZE * 2]; // 2 bytes per word

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            // Convert bytes to u16 words (little-endian)
            let words: Vec<u16> = buffer[..bytes_read]
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();

            self.decode_buffer(&words, &mut cd_events, &mut trigger_events);
        }

        Ok(DecodeResult {
            cd_events,
            trigger_events,
            metadata: self.metadata.clone(),
        })
    }

    /// Parses the file header to extract metadata.
    fn parse_header<R: BufRead>(&mut self, reader: &mut R) -> Result<(), DecodeError> {
        // EVT3 files may have a text header starting with '%'
        // We need to carefully peek and read line by line

        loop {
            let bytes_peeked = reader.fill_buf()?;

            if bytes_peeked.is_empty() {
                break;
            }

            if bytes_peeked[0] != b'%' {
                // No more header lines
                break;
            }

            // Read the full line
            let mut line = String::new();
            reader.read_line(&mut line)?;

            if line.starts_with("% end") {
                break;
            }

            self.parse_header_line(&line);
        }

        Ok(())
    }

    /// Parses a single header line.
    fn parse_header_line(&mut self, line: &str) {
        let line = line.trim_end();

        if let Some(format_str) = line.strip_prefix("% format ") {
            // Format: "% format EVT3;width=1280;height=720"
            for part in format_str.split(';') {
                if let Some(idx) = part.find('=') {
                    let name = &part[..idx];
                    let value = &part[idx + 1..];
                    match name {
                        "width" => {
                            if let Ok(w) = value.parse() {
                                self.metadata.width = w;
                            }
                        }
                        "height" => {
                            if let Ok(h) = value.parse() {
                                self.metadata.height = h;
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else if let Some(geometry_str) = line.strip_prefix("% geometry ") {
            // Format: "% geometry 1280x720"
            if let Some(idx) = geometry_str.find('x') {
                if let (Ok(w), Ok(h)) =
                    (geometry_str[..idx].parse(), geometry_str[idx + 1..].parse())
                {
                    self.metadata.width = w;
                    self.metadata.height = h;
                }
            }
        } else if let Some(version) = line.strip_prefix("% evt ") {
            // Format version check: "% evt 3.0"
            if version != "3.0" {
                // Could log a warning here, but we'll try to decode anyway
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoder_initial_state() {
        let decoder = Evt3Decoder::new();
        assert!(!decoder.first_time_base_set);
        assert_eq!(decoder.current_time, 0);
        assert_eq!(decoder.current_y, 0);
    }

    #[test]
    fn test_decode_simple_sequence() {
        let mut decoder = Evt3Decoder::new();
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();

        // Build a simple EVT3 sequence:
        // 1. TIME_HIGH with value 0
        // 2. TIME_LOW with value 100
        // 3. ADDR_Y with y=50
        // 4. ADDR_X with x=100, pol=1
        let words: Vec<u16> = vec![
            0x8000, // TIME_HIGH: type=8, time=0
            0x6064, // TIME_LOW: type=6, time=100
            0x0032, // ADDR_Y: type=0, y=50
            0x2864, // ADDR_X: type=2, pol=1, x=100
        ];

        decoder.decode_buffer(&words, &mut cd_events, &mut trigger_events);

        assert_eq!(cd_events.len(), 1);
        assert_eq!(cd_events[0].x, 100);
        assert_eq!(cd_events[0].y, 50);
        assert_eq!(cd_events[0].polarity, 1);
        assert_eq!(cd_events[0].timestamp, 100);
    }

    #[test]
    fn test_decode_vector_events() {
        let mut decoder = Evt3Decoder::new();
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();

        // Sequence with vector events:
        // 1. TIME_HIGH with value 0
        // 2. TIME_LOW with value 200
        // 3. ADDR_Y with y=100
        // 4. VECT_BASE_X with x=0, pol=0
        // 5. VECT_12 with valid=0b111000111000 (events at x=3,4,5,9,10,11)
        let words: Vec<u16> = vec![
            0x8000, // TIME_HIGH
            0x60C8, // TIME_LOW: 200
            0x0064, // ADDR_Y: y=100
            0x3000, // VECT_BASE_X: x=0, pol=0
            0x4E38, // VECT_12: valid=0b111000111000
        ];

        decoder.decode_buffer(&words, &mut cd_events, &mut trigger_events);

        assert_eq!(cd_events.len(), 6);

        // Check x coordinates (from validity mask)
        let x_coords: Vec<u16> = cd_events.iter().map(|e| e.x).collect();
        assert_eq!(x_coords, vec![3, 4, 5, 9, 10, 11]);

        // All should have same y, polarity, timestamp
        for event in &cd_events {
            assert_eq!(event.y, 100);
            assert_eq!(event.polarity, 0);
            assert_eq!(event.timestamp, 200);
        }
    }

    #[test]
    fn test_parse_header_line_format() {
        let mut decoder = Evt3Decoder::new();
        decoder.parse_header_line("% format EVT3;width=640;height=480");
        assert_eq!(decoder.metadata.width, 640);
        assert_eq!(decoder.metadata.height, 480);
    }

    #[test]
    fn test_parse_header_line_geometry() {
        let mut decoder = Evt3Decoder::new();
        decoder.parse_header_line("% geometry 320x240");
        assert_eq!(decoder.metadata.width, 320);
        assert_eq!(decoder.metadata.height, 240);
    }
}
