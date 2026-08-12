//! Stateful EVT 3.0 decoder.
//!
//! This module implements the EVT 3.0 decoding state machine that tracks
//! timestamp, coordinates, and polarity across events.

use crate::parser;
use crate::sink::{ColumnarEventSink, EventSink, VecEventSink};
use crate::types::{
    CdEvent, ColumnarDecodeResult, DecodeResult, RawEventType, SensorMetadata, TriggerEvent,
};
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

    #[cfg(feature = "hdf5")]
    #[error("HDF5 error: {0}")]
    Hdf5(#[from] hdf5::Error),

    #[cfg(feature = "hdf5")]
    #[error("HDF5 file missing required group: {0}")]
    MissingGroup(String),

    #[cfg(feature = "hdf5")]
    #[error("HDF5 geometry attribute malformed: {0}")]
    MalformedGeometry(String),
}

/// Constants for timestamp handling (matching C++ reference).
const MAX_TIMESTAMP_BASE: u64 = ((1u64 << 12) - 1) << 12; // 16773120us
const TIME_LOOP: u64 = MAX_TIMESTAMP_BASE + (1 << 12); // 16777216us
const LOOP_THRESHOLD: u64 = 10 << 12; // Threshold for loop detection

/// Buffer size for reading raw data (number of 16-bit words).
const READ_BUFFER_SIZE: usize = 1_000_000;

fn reserve_estimated_remaining_events<S: EventSink>(
    path: &Path,
    sample_bytes: usize,
    sink: &mut S,
    initial_len: usize,
) {
    let emitted = sink.cd_len().saturating_sub(initial_len);
    if emitted == 0 || sample_bytes == 0 {
        return;
    }

    let Ok(file_size) = std::fs::metadata(path).map(|metadata| metadata.len() as usize) else {
        return;
    };

    // Estimate from the first 2 MB and add a small margin. Clamp the estimate
    // to the physical maximum of one decoded event per input word.
    let estimated = (emitted as u128)
        .saturating_mul(file_size as u128)
        .saturating_mul(105)
        / (sample_bytes as u128 * 100);
    let estimated = usize::try_from(estimated)
        .unwrap_or(usize::MAX)
        .min(file_size / 2);
    let additional = estimated.saturating_sub(sink.cd_len());
    sink.reserve_cd(additional);
}

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

    // Byte-stream state
    pending_byte: Option<u8>,
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
            pending_byte: None,
            metadata: SensorMetadata::default(),
        }
    }

    /// Resets the decoder state.
    ///
    /// This clears the incremental event-decoding state and any buffered byte
    /// carried across [`Self::decode_bytes`] calls. Parsed metadata is left
    /// unchanged.
    pub fn reset(&mut self) {
        self.time_base = 0;
        self.time_low = 0;
        self.current_time = 0;
        self.n_time_high_loops = 0;
        self.first_time_base_set = false;
        self.current_y = 0;
        self.current_base_x = 0;
        self.current_polarity = 0;
        self.pending_byte = None;
    }

    /// Decodes a buffer of 16-bit words into CD and trigger events.
    ///
    /// This is the core decoding function that processes raw EVT 3.0 words.
    /// Use [`Self::decode_bytes`] for raw byte streams.
    pub fn decode_buffer(
        &mut self,
        words: &[u16],
        cd_events: &mut Vec<CdEvent>,
        trigger_events: &mut Vec<TriggerEvent>,
    ) {
        let mut sink = VecEventSink {
            cd: cd_events,
            triggers: trigger_events,
        };
        self.decode_buffer_into(words, &mut sink);
    }

    /// Decodes 16-bit EVT3 words directly into a caller-selected storage layout.
    pub fn decode_buffer_into<S: EventSink>(&mut self, words: &[u16], sink: &mut S) {
        for &word in words {
            self.decode_word_into(word, sink);
        }
    }

    /// Decodes raw little-endian EVT 3.0 bytes into CD and trigger events.
    ///
    /// This method is incremental and stateful. It accepts arbitrary chunk
    /// boundaries, including odd-length chunks, and buffers a trailing byte
    /// until the next call or [`Self::finish_stream`].
    pub fn decode_bytes(
        &mut self,
        bytes: &[u8],
        cd_events: &mut Vec<CdEvent>,
        trigger_events: &mut Vec<TriggerEvent>,
    ) -> Result<(), DecodeError> {
        let mut sink = VecEventSink {
            cd: cd_events,
            triggers: trigger_events,
        };
        self.decode_bytes_into(bytes, &mut sink);
        Ok(())
    }

    /// Decodes raw little-endian bytes directly into a caller-selected sink.
    ///
    /// Unlike the legacy implementation, this does not build an intermediate
    /// `Vec<u16>` for every byte chunk.
    pub fn decode_bytes_into<S: EventSink>(&mut self, bytes: &[u8], sink: &mut S) {
        const WORD_BATCH: usize = 4096;
        let mut remaining = bytes;

        if let Some(pending) = self.pending_byte.take() {
            if let Some((&next, rest)) = remaining.split_first() {
                self.decode_word_into(u16::from_le_bytes([pending, next]), sink);
                remaining = rest;
            } else {
                self.pending_byte = Some(pending);
                return;
            }
        }

        let even_bytes = remaining.len() & !1;
        let mut offset = 0;
        let mut words = [0_u16; WORD_BATCH];

        while offset < even_bytes {
            let word_count = ((even_bytes - offset) / 2).min(WORD_BATCH);
            let batch_end = offset + word_count * 2;
            for (word, bytes) in words[..word_count]
                .iter_mut()
                .zip(remaining[offset..batch_end].chunks_exact(2))
            {
                *word = u16::from_le_bytes([bytes[0], bytes[1]]);
            }
            self.decode_buffer_into(&words[..word_count], sink);
            offset = batch_end;
        }

        if even_bytes != remaining.len() {
            self.pending_byte = remaining.last().copied();
        }
    }

    #[inline(always)]
    fn decode_word_into<S: EventSink>(&mut self, word: u16, sink: &mut S) {
        let event_type = parser::get_event_type(word);

        if !self.first_time_base_set {
            if event_type == RawEventType::TimeHigh as u8 {
                let time_val = parser::time_get_value(word);
                self.time_base = (time_val as u64) << 12;
                self.current_time = self.time_base;
                self.first_time_base_set = true;
            }
            return;
        }

        match event_type {
            0x2 => sink.push_cd(
                parser::addr_x_get_x(word),
                self.current_y,
                parser::addr_x_get_polarity(word),
                self.current_time,
            ),
            0x4 => {
                self.process_vector_events(parser::vect_12_get_valid(word) as u32, 12, sink);
            }
            0x5 => {
                self.process_vector_events(parser::vect_8_get_valid(word) as u32, 8, sink);
            }
            0x0 => {
                self.current_y = parser::addr_y_get_y(word);
            }
            0x3 => {
                self.current_base_x = parser::vect_base_x_get_x(word);
                self.current_polarity = parser::vect_base_x_get_polarity(word);
            }
            0x8 => self.process_time_high(word),
            0x6 => {
                self.time_low = parser::time_get_value(word) as u64;
                self.current_time = self.time_base + self.time_low;
            }
            0xA => sink.push_trigger(
                parser::ext_trigger_get_value(word),
                parser::ext_trigger_get_id(word),
                self.current_time,
            ),
            _ => {}
        }
    }

    /// Finalizes a byte stream previously fed through [`Self::decode_bytes`].
    ///
    /// Call this only when the input stream is known to be complete. It returns
    /// [`DecodeError::UnexpectedEof`] if a single trailing byte is still
    /// buffered.
    pub fn finish_stream(&mut self) -> Result<(), DecodeError> {
        if self.pending_byte.take().is_some() {
            return Err(DecodeError::UnexpectedEof);
        }

        Ok(())
    }

    /// Finalizes a byte stream leniently, discarding any buffered trailing
    /// padding byte.
    ///
    /// Use this at file EOF when a stray byte is known to be benign, such as
    /// legacy `.raw` files that end with a newline after the binary payload.
    pub fn finish_stream_lenient(&mut self) {
        self.pending_byte = None;
    }

    #[inline]
    pub(crate) fn discard_trailing_file_padding(&mut self) {
        if matches!(self.pending_byte, Some(b'\n' | b'\r')) {
            self.pending_byte = None;
        }
    }

    /// Processes TIME_HIGH events with loop detection.
    #[inline(always)]
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
    #[inline(always)]
    fn process_vector_events<S: EventSink>(&mut self, mut valid: u32, count: u16, sink: &mut S) {
        let end_x = self.current_base_x + count;

        while valid != 0 {
            let bit = valid.trailing_zeros() as u16;
            sink.push_cd(
                self.current_base_x + bit,
                self.current_y,
                self.current_polarity,
                self.current_time,
            );
            valid &= valid - 1;
        }

        self.current_base_x = end_x;
    }

    /// Decodes an EVT 3.0 file from disk.
    ///
    /// Parses the file header (if present) and decodes all events.
    pub fn decode_file<P: AsRef<Path>>(&mut self, path: P) -> Result<DecodeResult, DecodeError> {
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();
        {
            let mut sink = VecEventSink {
                cd: &mut cd_events,
                triggers: &mut trigger_events,
            };
            self.decode_file_into(path, &mut sink)?;
        }

        Ok(DecodeResult {
            cd_events,
            trigger_events,
            metadata: self.metadata.clone(),
        })
    }

    /// Decodes a file directly into columnar storage.
    pub fn decode_file_columns<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<ColumnarDecodeResult, DecodeError> {
        let mut sink = ColumnarEventSink::default();
        self.decode_file_into(path, &mut sink)?;
        Ok(ColumnarDecodeResult {
            cd_events: sink.cd,
            trigger_events: sink.triggers,
            metadata: self.metadata.clone(),
        })
    }

    /// Decodes a file into a caller-selected event storage layout.
    pub fn decode_file_into<P: AsRef<Path>, S: EventSink>(
        &mut self,
        path: P,
        sink: &mut S,
    ) -> Result<(), DecodeError> {
        let path = path.as_ref();
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        #[cfg(feature = "hdf5")]
        if matches!(extension.as_str(), "h5" | "hdf5") {
            return crate::hdf5_decoder::decode_hdf5_into(self, path, sink);
        }

        #[cfg(not(feature = "hdf5"))]
        if matches!(extension.as_str(), "h5" | "hdf5") {
            return Err(DecodeError::InvalidFormat(
                "HDF5 input requires building with the 'hdf5' cargo feature".to_string(),
            ));
        }

        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        // Parse header
        self.parse_header(&mut reader)?;

        // Read and decode raw data
        let mut buffer = vec![0u8; READ_BUFFER_SIZE * 2]; // 2 bytes per word
        let mut first_chunk = true;

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            let before = sink.cd_len();
            self.decode_bytes_into(&buffer[..bytes_read], sink);

            if first_chunk {
                reserve_estimated_remaining_events(path, bytes_read, sink, before);
                first_chunk = false;
            }
        }

        // Some recorded .raw files include a trailing newline byte after the
        // binary payload. Preserve historical file-decoding behavior by
        // ignoring that single byte here while keeping finish_stream strict for
        // raw byte-stream callers.
        self.discard_trailing_file_padding();
        self.finish_stream()?;

        Ok(())
    }

    /// Parses the file header to extract metadata.
    pub(crate) fn parse_header<R: BufRead>(&mut self, reader: &mut R) -> Result<(), DecodeError> {
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
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn sample_words() -> Vec<u16> {
        vec![
            0x8000, // TIME_HIGH
            0x6064, // TIME_LOW: 100
            0x0032, // ADDR_Y: y=50
            0x2864, // ADDR_X: x=100, pol=1
            0xA801, // EXT_TRIGGER: value=1, id=0
            0x3008, // VECT_BASE_X: x=8, pol=0
            0x500D, // VECT_8: valid bits at x=8,10,11
            0x6078, // TIME_LOW: 120
            0x280C, // ADDR_X: x=12, pol=1
        ]
    }

    fn words_to_bytes(words: &[u16]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    fn decode_words(words: &[u16]) -> (Vec<CdEvent>, Vec<TriggerEvent>) {
        let mut decoder = Evt3Decoder::new();
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();
        decoder.decode_buffer(words, &mut cd_events, &mut trigger_events);
        (cd_events, trigger_events)
    }

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
    fn columnar_decode_matches_legacy_layout() {
        let words = sample_words();
        let (expected_cd, expected_triggers) = decode_words(&words);
        let mut decoder = Evt3Decoder::new();
        let mut sink = ColumnarEventSink::default();

        decoder.decode_buffer_into(&words, &mut sink);

        assert_eq!(sink.cd.len(), expected_cd.len());
        for (index, expected) in expected_cd.iter().enumerate() {
            assert_eq!(sink.cd.x[index], expected.x);
            assert_eq!(sink.cd.y[index], expected.y);
            assert_eq!(sink.cd.polarity[index], expected.polarity);
            assert_eq!(sink.cd.timestamp[index], expected.timestamp);
        }
        assert_eq!(sink.triggers.len(), expected_triggers.len());
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

    #[test]
    fn decode_bytes_matches_decode_buffer_for_simple_sequence() {
        let words = sample_words();
        let bytes = words_to_bytes(&words);
        let (expected_cd, expected_triggers) = decode_words(&words);

        let mut decoder = Evt3Decoder::new();
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();

        decoder
            .decode_bytes(&bytes, &mut cd_events, &mut trigger_events)
            .unwrap();
        decoder.finish_stream().unwrap();

        assert_eq!(cd_events, expected_cd);
        assert_eq!(trigger_events, expected_triggers);
    }

    #[test]
    fn decode_bytes_handles_odd_chunk_boundary() {
        let words = sample_words();
        let bytes = words_to_bytes(&words);
        let split = 5;
        let (expected_cd, expected_triggers) = decode_words(&words);

        let mut decoder = Evt3Decoder::new();
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();

        decoder
            .decode_bytes(&bytes[..split], &mut cd_events, &mut trigger_events)
            .unwrap();
        decoder
            .decode_bytes(&bytes[split..], &mut cd_events, &mut trigger_events)
            .unwrap();
        decoder.finish_stream().unwrap();

        assert_eq!(cd_events, expected_cd);
        assert_eq!(trigger_events, expected_triggers);
    }

    #[test]
    fn decode_bytes_handles_multiple_small_chunks() {
        let words = sample_words();
        let bytes = words_to_bytes(&words);
        let chunk_sizes = [1usize, 3, 5, 7];
        let (expected_cd, expected_triggers) = decode_words(&words);

        let mut decoder = Evt3Decoder::new();
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();
        let mut offset = 0usize;
        let mut chunk_index = 0usize;

        while offset < bytes.len() {
            let chunk_size = chunk_sizes[chunk_index % chunk_sizes.len()];
            let end = (offset + chunk_size).min(bytes.len());
            decoder
                .decode_bytes(&bytes[offset..end], &mut cd_events, &mut trigger_events)
                .unwrap();
            offset = end;
            chunk_index += 1;
        }

        decoder.finish_stream().unwrap();

        assert_eq!(cd_events, expected_cd);
        assert_eq!(trigger_events, expected_triggers);
    }

    #[test]
    fn finish_stream_errors_on_dangling_half_word() {
        let mut decoder = Evt3Decoder::new();
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();

        decoder
            .decode_bytes(&[0x00], &mut cd_events, &mut trigger_events)
            .unwrap();

        assert!(matches!(
            decoder.finish_stream(),
            Err(DecodeError::UnexpectedEof)
        ));
    }

    #[test]
    fn finish_stream_succeeds_on_even_boundary() {
        let words = sample_words();
        let bytes = words_to_bytes(&words);
        let mut decoder = Evt3Decoder::new();
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();

        decoder
            .decode_bytes(&bytes, &mut cd_events, &mut trigger_events)
            .unwrap();

        assert!(decoder.finish_stream().is_ok());
    }

    #[test]
    fn finish_stream_lenient_discards_dangling_half_word() {
        let mut decoder = Evt3Decoder::new();
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();

        decoder
            .decode_bytes(&[0x00], &mut cd_events, &mut trigger_events)
            .unwrap();

        decoder.finish_stream_lenient();
        assert!(decoder.finish_stream().is_ok());
    }

    #[test]
    fn reset_clears_pending_byte() {
        let words = sample_words();
        let bytes = words_to_bytes(&words);
        let (expected_cd, expected_triggers) = decode_words(&words);

        let mut decoder = Evt3Decoder::new();
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();

        decoder
            .decode_bytes(&bytes[..1], &mut cd_events, &mut trigger_events)
            .unwrap();
        decoder.reset();
        decoder
            .decode_bytes(&bytes, &mut cd_events, &mut trigger_events)
            .unwrap();
        decoder.finish_stream().unwrap();

        assert_eq!(cd_events, expected_cd);
        assert_eq!(trigger_events, expected_triggers);
    }

    #[test]
    fn decode_file_still_decodes_existing_sequences() {
        let words = sample_words();
        let bytes = words_to_bytes(&words);
        let (expected_cd, expected_triggers) = decode_words(&words);
        let mut file = NamedTempFile::new().unwrap();

        writeln!(file, "% format EVT3;width=640;height=480").unwrap();
        writeln!(file, "% end").unwrap();
        file.write_all(&bytes).unwrap();
        file.flush().unwrap();

        let mut decoder = Evt3Decoder::new();
        let result = decoder.decode_file(file.path()).unwrap();

        assert_eq!(result.metadata.width, 640);
        assert_eq!(result.metadata.height, 480);
        assert_eq!(result.cd_events, expected_cd);
        assert_eq!(result.trigger_events, expected_triggers);
    }

    #[test]
    fn decode_file_ignores_trailing_newline_padding() {
        let words = sample_words();
        let bytes = words_to_bytes(&words);
        let (expected_cd, expected_triggers) = decode_words(&words);
        let mut file = NamedTempFile::new().unwrap();

        writeln!(file, "% format EVT3;width=640;height=480").unwrap();
        writeln!(file, "% end").unwrap();
        file.write_all(&bytes).unwrap();
        file.write_all(b"\n").unwrap();
        file.flush().unwrap();

        let mut decoder = Evt3Decoder::new();
        let result = decoder.decode_file(file.path()).unwrap();

        assert_eq!(result.cd_events, expected_cd);
        assert_eq!(result.trigger_events, expected_triggers);
    }
}
