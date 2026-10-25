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

mod counts;
#[cfg(feature = "parallel")]
mod parallel;

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
const READ_BUFFER_SIZE: usize = 4_000_000;

trait WordSource {
    fn next_word(&mut self) -> Option<u16>;
    fn take_vector_tail(&mut self) -> Option<u32>;
    fn take_vector_continuation(&mut self) -> Option<u32>;
}

#[inline(always)]
fn vector_tail(first: u16, second: u16, third: u16) -> Option<u32> {
    if ((first & 0xf000) ^ 0x4000) | ((second & 0xf000) ^ 0x4000) | ((third & 0xf000) ^ 0x5000) != 0
    {
        return None;
    }
    Some(
        u32::from(first & 0x0fff)
            | (u32::from(second & 0x0fff) << 12)
            | (u32::from(third & 0x00ff) << 24),
    )
}

struct WordSlice<'a>(&'a [u16]);

impl WordSource for WordSlice<'_> {
    #[inline(always)]
    fn next_word(&mut self) -> Option<u16> {
        let (&word, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(word)
    }

    #[inline(always)]
    fn take_vector_tail(&mut self) -> Option<u32> {
        let [first, second, third, rest @ ..] = self.0 else {
            return None;
        };
        let valid = vector_tail(*first, *second, *third)?;
        self.0 = rest;
        Some(valid)
    }
    #[inline(always)]
    fn take_vector_continuation(&mut self) -> Option<u32> {
        let [first, second, rest @ ..] = self.0 else {
            return None;
        };
        let packed = u32::from(*first) | (u32::from(*second) << 16);
        if packed & 0xf000_f000 != 0x5000_4000 {
            return None;
        }
        self.0 = rest;
        Some((packed & 0x0fff) | ((packed >> 4) & 0x000ff000))
    }
}

struct ByteWords<'a>(&'a [u8]);

impl WordSource for ByteWords<'_> {
    #[inline(always)]
    fn next_word(&mut self) -> Option<u16> {
        let [low, high, rest @ ..] = self.0 else {
            return None;
        };
        self.0 = rest;
        Some(u16::from_le_bytes([*low, *high]))
    }

    #[inline(always)]
    fn take_vector_tail(&mut self) -> Option<u32> {
        let packed = if let Some(bytes) = self.0.get(..8) {
            u64::from_le_bytes(bytes.try_into().unwrap())
        } else {
            let [a, b, c, d, e, f, ..] = self.0 else {
                return None;
            };
            u64::from_le_bytes([*a, *b, *c, *d, *e, *f, 0, 0])
        };
        if packed & 0x0000_f000_f000_f000 != 0x0000_5000_4000_4000 {
            return None;
        }
        self.0 = &self.0[6..];
        Some(
            ((packed & 0x0fff) | ((packed >> 4) & 0x00fff000) | ((packed >> 8) & 0xff000000))
                as u32,
        )
    }
    #[inline(always)]
    fn take_vector_continuation(&mut self) -> Option<u32> {
        let packed = u32::from_le_bytes(self.0.get(..4)?.try_into().unwrap());
        if packed & 0xf000_f000 != 0x5000_4000 {
            return None;
        }
        self.0 = &self.0[4..];
        Some((packed & 0x0fff) | ((packed >> 4) & 0x000ff000))
    }
}

fn reserve_estimated_remaining_events<S: EventSink>(
    path: &Path,
    sample_bytes: usize,
    sink: &mut S,
    emitted: usize,
    already_emitted: usize,
) {
    if emitted == 0 || sample_bytes == 0 {
        return;
    }

    let Ok(file_size) = std::fs::metadata(path).map(|metadata| metadata.len() as usize) else {
        return;
    };

    // Estimate from the initial sample with a margin. Limit speculative
    // reservation to half the input byte size; denser output grows normally.
    let estimated = (emitted as u128)
        .saturating_mul(file_size as u128)
        .saturating_mul(105)
        / (sample_bytes as u128 * 100);
    let estimated = usize::try_from(estimated)
        .unwrap_or(usize::MAX)
        .min(file_size / 2);
    let additional = estimated.saturating_sub(already_emitted);
    sink.reserve_cd(additional);
}

/// Maximum CPU parallelism used by concrete row/column decoding APIs.
///
/// Generic [`Evt3Decoder::decode_bytes_into`] and
/// [`Evt3Decoder::decode_file_into`] always run on the calling thread.
/// Without the `parallel` Cargo feature, all settings execute serially.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Parallelism {
    /// Decode on the calling thread. This is the Rust library default.
    #[default]
    Off,
    /// Use up to four total decoding threads, adapting to concurrent calls.
    Auto,
    /// Use at most this many total decoding threads, including the caller.
    /// Values zero and one execute serially; the CPU count is also an upper bound.
    Threads(usize),
}

/// Stateful EVT 3.0 decoder.
///
/// Maintains internal state to properly reconstruct the event stream according
/// to the EVT 3.0 specification.
#[derive(Debug)]
pub struct Evt3Decoder {
    parallelism: Parallelism,
    #[cfg(feature = "parallel")]
    parallel_pool: Option<std::sync::Arc<rayon::ThreadPool>>,
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

// Keep the working state independent of output pointers in the hot loop,
// writing partial progress back even if a custom sink unwinds.
struct DecoderStateGuard<'a> {
    target: &'a mut Evt3Decoder,
    local: Evt3Decoder,
}

impl Drop for DecoderStateGuard<'_> {
    fn drop(&mut self) {
        std::mem::swap(self.target, &mut self.local);
    }
}

impl Default for Evt3Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Evt3Decoder {
    /// Creates a decoder with [`Parallelism::Off`], even when `parallel` is built.
    pub fn new() -> Self {
        Self {
            parallelism: Parallelism::Off,
            #[cfg(feature = "parallel")]
            parallel_pool: None,
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

    /// Selects the execution policy for concrete row/column APIs.
    pub fn with_parallelism(mut self, parallelism: Parallelism) -> Self {
        self.set_parallelism(parallelism);
        self
    }

    /// Changes execution policy without changing stream state or metadata.
    pub fn set_parallelism(&mut self, parallelism: Parallelism) {
        if self.parallelism != parallelism {
            #[cfg(feature = "parallel")]
            {
                self.parallel_pool = None;
            }
            self.parallelism = parallelism;
        }
    }

    /// Returns the configured policy. It survives [`Self::reset`].
    pub fn parallelism(&self) -> Parallelism {
        self.parallelism
    }

    pub(super) fn snapshot(&self) -> Self {
        Self {
            parallelism: self.parallelism,
            #[cfg(feature = "parallel")]
            parallel_pool: self.parallel_pool.clone(),
            time_base: self.time_base,
            time_low: self.time_low,
            current_time: self.current_time,
            n_time_high_loops: self.n_time_high_loops,
            first_time_base_set: self.first_time_base_set,
            current_y: self.current_y,
            current_base_x: self.current_base_x,
            current_polarity: self.current_polarity,
            pending_byte: self.pending_byte,
            metadata: self.metadata.clone(),
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
        sink.decode_word_chunk(self, words);
    }

    /// Decodes 16-bit EVT3 words directly into a caller-selected storage layout.
    pub fn decode_buffer_into<S: EventSink>(&mut self, words: &[u16], sink: &mut S) {
        self.decode_buffer_scalar(words, sink);
    }

    pub(crate) fn decode_buffer_scalar<S: EventSink>(&mut self, words: &[u16], sink: &mut S) {
        self.decode_words_into(WordSlice(words), sink);
    }

    #[inline(always)]
    fn decode_words_into<S: EventSink>(&mut self, words: impl WordSource, sink: &mut S) {
        let local = self.snapshot();
        let mut guard = DecoderStateGuard {
            target: self,
            local,
        };
        guard.local.decode_words_local(words, sink);
    }

    #[inline(always)]
    fn decode_words_local<S: EventSink>(&mut self, mut words: impl WordSource, sink: &mut S) {
        // Synchronize once per input chunk instead of testing initialization
        // for every word in the steady-state loop.
        if !self.first_time_base_set {
            loop {
                let Some(word) = words.next_word() else {
                    return;
                };
                if parser::get_event_type(word) == RawEventType::TimeHigh as u8 {
                    self.time_base = (parser::time_get_value(word) as u64) << 12;
                    self.current_time = self.time_base;
                    self.first_time_base_set = true;
                    break;
                }
            }
        }

        while let Some(word) = words.next_word() {
            self.decode_word_into(word, &mut words, sink);
        }
    }

    /// Decodes raw little-endian EVT 3.0 bytes into CD and trigger events.
    ///
    /// This method is incremental and stateful. It accepts arbitrary chunk
    /// boundaries, including odd-length chunks, and buffers a trailing byte
    /// until the next call or [`Self::finish_stream`].
    /// This concrete row API honors the configured [`Parallelism`].
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
        self.decode_rows_into(bytes, &mut sink);
        Ok(())
    }

    /// Decodes raw little-endian bytes directly into a caller-selected sink.
    ///
    /// Converts byte pairs and decodes them in one pass without an intermediate
    /// word buffer. This generic path always runs serially, regardless of
    /// the configured parallelism or sink type.
    pub fn decode_bytes_into<S: EventSink>(&mut self, bytes: &[u8], sink: &mut S) {
        self.decode_bytes_scalar(bytes, sink);
    }

    fn decode_rows_into(&mut self, bytes: &[u8], sink: &mut VecEventSink<'_>) {
        #[cfg(feature = "parallel")]
        if self.try_parallel_rows(bytes, sink.cd, sink.triggers) {
            return;
        }
        sink.decode_byte_chunk(self, bytes);
    }

    /// Decodes bytes into built-in columns using the configured execution policy.
    ///
    /// Unlike [`Self::decode_bytes_into`], this concrete API may use workers
    /// when explicitly enabled. Odd chunks and carried half-words remain serial.
    pub fn decode_bytes_columns(&mut self, bytes: &[u8], sink: &mut ColumnarEventSink) {
        #[cfg(feature = "parallel")]
        if self.try_parallel_columns(bytes, sink) {
            return;
        }
        sink.decode_byte_chunk(self, bytes);
    }

    pub(crate) fn decode_bytes_scalar<S: EventSink>(&mut self, bytes: &[u8], sink: &mut S) {
        let mut remaining = bytes;

        if let Some(pending) = self.pending_byte.take() {
            if let Some((&next, rest)) = remaining.split_first() {
                self.decode_buffer_scalar(&[u16::from_le_bytes([pending, next])], sink);
                remaining = rest;
            } else {
                self.pending_byte = Some(pending);
                return;
            }
        }

        let even_bytes = remaining.len() & !1;
        self.decode_words_into(ByteWords(&remaining[..even_bytes]), sink);
        self.pending_byte = remaining.get(even_bytes).copied();
    }

    #[inline(always)]
    fn decode_word_into<S: EventSink>(
        &mut self,
        word: u16,
        words: &mut impl WordSource,
        sink: &mut S,
    ) {
        let event_type = parser::get_event_type(word);

        match event_type {
            0x2 => sink.push_cd(
                parser::addr_x_get_x(word),
                self.current_y,
                parser::addr_x_get_polarity(word),
                self.current_time,
            ),
            0x4 => {
                let valid = u32::from(parser::vect_12_get_valid(word));
                if let Some(tail) = words.take_vector_continuation() {
                    self.process_vector_events(valid | (tail << 12), 32, sink);
                } else {
                    self.process_vector_events(valid, 12, sink);
                }
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
                if let Some(valid) = words.take_vector_tail() {
                    self.process_vector_events(valid, 32, sink);
                }
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
    /// Uses the configured execution policy; the Rust default is serial.
    pub fn decode_file<P: AsRef<Path>>(&mut self, path: P) -> Result<DecodeResult, DecodeError> {
        let mut cd_events = Vec::new();
        let mut trigger_events = Vec::new();
        {
            let mut sink = VecEventSink {
                cd: &mut cd_events,
                triggers: &mut trigger_events,
            };
            self.decode_file_with(path.as_ref(), &mut sink, Self::decode_rows_into)?;
        }

        Ok(DecodeResult {
            cd_events,
            trigger_events,
            metadata: self.metadata.clone(),
        })
    }

    /// Decodes a file directly into columns using the configured execution policy.
    pub fn decode_file_columns<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<ColumnarDecodeResult, DecodeError> {
        let mut sink = ColumnarEventSink::default();
        self.decode_file_with(path.as_ref(), &mut sink, Self::decode_bytes_columns)?;
        Ok(ColumnarDecodeResult {
            cd_events: sink.cd,
            trigger_events: sink.triggers,
            metadata: self.metadata.clone(),
        })
    }

    /// Decodes a file into a caller-selected event storage layout, serially.
    pub fn decode_file_into<P: AsRef<Path>, S: EventSink>(
        &mut self,
        path: P,
        sink: &mut S,
    ) -> Result<(), DecodeError> {
        self.decode_file_with(path.as_ref(), sink, Self::decode_bytes_into)
    }

    fn decode_file_with<S: EventSink>(
        &mut self,
        path: &Path,
        sink: &mut S,
        mut decode: impl FnMut(&mut Self, &[u8], &mut S),
    ) -> Result<(), DecodeError> {
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

            let input = &buffer[..bytes_read];
            let mut reserved_before_decode = false;
            if first_chunk && self.pending_byte.is_none() {
                let synchronized = if self.first_time_base_set {
                    Some(input)
                } else {
                    input
                        .chunks_exact(2)
                        .position(|word| word[1] >> 4 == 8)
                        .map(|offset| &input[offset * 2 + 2..])
                };
                if let Some(synchronized) = synchronized {
                    let (events, _) = self.output_counts(synchronized);
                    reserve_estimated_remaining_events(path, bytes_read, sink, events, 0);
                    reserved_before_decode = true;
                }
            }
            let before = sink.cd_len();
            decode(self, input, sink);

            if first_chunk {
                if !reserved_before_decode {
                    let emitted = sink.cd_len().saturating_sub(before);
                    reserve_estimated_remaining_events(path, bytes_read, sink, emitted, emitted);
                }
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

    fn policy_test_bytes() -> Vec<u8> {
        [
            0x8000_u16, 0x6001, 0x0007, 0x3803, 0x4fff, 0x4001, 0x5f81, 0xa301,
        ]
        .repeat(8192)
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect()
    }

    #[test]
    fn library_defaults_and_generic_sinks_never_start_workers() {
        let bytes = policy_test_bytes();
        let mut decoder = Evt3Decoder::default();
        assert_eq!(decoder.parallelism(), Parallelism::Off);
        let mut rows = Vec::new();
        let mut triggers = Vec::new();
        decoder
            .decode_bytes(&bytes, &mut rows, &mut triggers)
            .unwrap();
        #[cfg(feature = "parallel")]
        assert!(decoder.parallel_pool.is_none());

        decoder.reset();
        decoder.set_parallelism(Parallelism::Auto);
        let mut columns = ColumnarEventSink::default();
        // Even a built-in sink is serial through the generic entry point.
        decoder.decode_bytes_into(&bytes, &mut columns);
        assert_eq!(columns.cd.len(), rows.len());
        assert_eq!(columns.triggers.len(), triggers.len());
        #[cfg(feature = "parallel")]
        assert!(decoder.parallel_pool.is_none());

        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut file, &bytes).unwrap();
        columns.clear();
        decoder.reset();
        decoder.decode_file_into(file.path(), &mut columns).unwrap();
        assert_eq!(columns.cd.len(), rows.len());
        #[cfg(feature = "parallel")]
        assert!(decoder.parallel_pool.is_none());
    }

    #[test]
    fn explicit_policies_preserve_outputs_and_reset_keeps_configuration() {
        let bytes = policy_test_bytes();
        let mut reference = Evt3Decoder::new();
        let mut expected = ColumnarEventSink::default();
        reference.decode_bytes_columns(&bytes, &mut expected);
        for policy in [
            Parallelism::Off,
            Parallelism::Auto,
            Parallelism::Threads(0),
            Parallelism::Threads(1),
            Parallelism::Threads(2),
            Parallelism::Threads(4),
        ] {
            let mut decoder = Evt3Decoder::new().with_parallelism(policy);
            let mut actual = ColumnarEventSink::default();
            decoder.decode_bytes_columns(&bytes, &mut actual);
            assert_eq!(actual.cd.x, expected.cd.x);
            assert_eq!(actual.cd.y, expected.cd.y);
            assert_eq!(actual.cd.polarity, expected.cd.polarity);
            assert_eq!(actual.cd.timestamp, expected.cd.timestamp);
            assert_eq!(actual.triggers.timestamp, expected.triggers.timestamp);
            #[cfg(feature = "parallel")]
            if let Some(pool) = &decoder.parallel_pool {
                let cap = match policy {
                    Parallelism::Auto => 4,
                    Parallelism::Threads(n) => n,
                    Parallelism::Off => panic!("Off created a pool"),
                };
                assert!(pool.current_num_threads() < cap);
            }
            decoder.reset();
            assert_eq!(decoder.parallelism(), policy);
            let mut rows = Vec::new();
            let mut triggers = Vec::new();
            decoder
                .decode_bytes(&bytes, &mut rows, &mut triggers)
                .unwrap();
            for (i, event) in rows.iter().enumerate() {
                assert_eq!(
                    (event.x, event.y, event.polarity, event.timestamp),
                    (
                        expected.cd.x[i],
                        expected.cd.y[i],
                        expected.cd.polarity[i],
                        expected.cd.timestamp[i]
                    )
                );
            }
            assert_eq!(rows.len(), expected.cd.len());
            assert_eq!(triggers.len(), expected.triggers.len());
            decoder.set_parallelism(Parallelism::Off);
            #[cfg(feature = "parallel")]
            assert!(decoder.parallel_pool.is_none());
        }
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn small_or_irregular_chunks_do_not_create_a_pool() {
        let bytes = policy_test_bytes();
        for size in [4095, 8192, 65_535, 65_537] {
            let mut decoder = Evt3Decoder::new().with_parallelism(Parallelism::Auto);
            decoder.decode_bytes_columns(&bytes[..size], &mut ColumnarEventSink::default());
            assert!(decoder.parallel_pool.is_none());
        }
        let mut decoder = Evt3Decoder::new().with_parallelism(Parallelism::Auto);
        let mut columns = ColumnarEventSink::default();
        decoder.decode_bytes_columns(&bytes[..1], &mut columns);
        decoder.decode_bytes_columns(&bytes[1..65_537], &mut columns);
        assert!(decoder.parallel_pool.is_none());
    }
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
    fn addr_x_runs_preserve_polarity_state_and_chunk_boundaries() {
        let mut words = vec![0x8001, 0x6007, 0x0011];
        words.extend((0..4096_u16).map(|value| 0x2000 | value));
        words.extend([0x8002, 0x6009, 0x0013]);
        words.extend((0..4096_u16).rev().map(|value| 0x2000 | value));
        words.extend([0xa301, 0x3005, 0x4003]);
        let bytes = words_to_bytes(&words);
        let mut reference = Evt3Decoder::new();
        let (mut expected, mut expected_triggers) = (Vec::new(), Vec::new());
        for word in &words {
            reference.decode_buffer(
                std::slice::from_ref(word),
                &mut expected,
                &mut expected_triggers,
            );
        }
        assert_eq!(expected[0], CdEvent::new(0, 17, 0, 4103));
        assert_eq!(expected[4096], CdEvent::new(2047, 19, 1, 8201));
        for size in [1, 2, 3, 4, 5, 7, 8, 31, 4095, 8192, bytes.len()] {
            let mut decoder = Evt3Decoder::new();
            let (mut actual, mut triggers) = (Vec::new(), Vec::new());
            for chunk in bytes.chunks(size) {
                decoder
                    .decode_bytes(chunk, &mut actual, &mut triggers)
                    .unwrap();
            }
            decoder.finish_stream().unwrap();
            assert_eq!(actual, expected);
            assert_eq!(triggers, expected_triggers);
        }
        assert_eq!(decode_words(&words), (expected, expected_triggers));
    }

    #[test]
    fn test_decoder_initial_state() {
        let decoder = Evt3Decoder::new();
        assert!(!decoder.first_time_base_set);
        assert_eq!(decoder.current_time, 0);
        assert_eq!(decoder.current_y, 0);
    }

    #[test]
    fn a_panicking_custom_sink_keeps_partial_decoder_progress() {
        struct StopAtSecondEvent(usize);
        impl EventSink for StopAtSecondEvent {
            fn cd_len(&self) -> usize {
                self.0
            }
            fn trigger_len(&self) -> usize {
                0
            }
            fn push_cd(&mut self, _: u16, _: u16, _: u8, _: u64) {
                self.0 += 1;
                assert!(self.0 < 2, "stop this consumer");
            }
            fn push_trigger(&mut self, _: u8, _: u8, _: u64) {}
        }
        let mut decoder = Evt3Decoder::new();
        let mut sink = StopAtSecondEvent(0);
        let words = [
            0x8001, 0x6123, 0x002a, 0x3809, 0x4001, 0x002b, 0x6064, 0x2002,
        ];
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            decoder.decode_buffer_into(&words, &mut sink);
        }))
        .is_err());
        assert_eq!(decoder.current_base_x, 21);
        let mut events = Vec::new();
        decoder.decode_buffer(&[0x2007], &mut events, &mut Vec::new());
        assert_eq!(events, [CdEvent::new(7, 43, 0, 0x1064)]);
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

    fn assert_columns_match(sink: &ColumnarEventSink, cd: &[CdEvent], triggers: &[TriggerEvent]) {
        assert_eq!(sink.cd.x, cd.iter().map(|e| e.x).collect::<Vec<_>>());
        assert_eq!(sink.cd.y, cd.iter().map(|e| e.y).collect::<Vec<_>>());
        assert_eq!(
            sink.cd.polarity,
            cd.iter().map(|e| e.polarity).collect::<Vec<_>>()
        );
        assert_eq!(
            sink.cd.timestamp,
            cd.iter().map(|e| e.timestamp).collect::<Vec<_>>()
        );
        assert_eq!(
            sink.triggers.value,
            triggers.iter().map(|e| e.value).collect::<Vec<_>>()
        );
        assert_eq!(
            sink.triggers.id,
            triggers.iter().map(|e| e.id).collect::<Vec<_>>()
        );
        assert_eq!(
            sink.triggers.timestamp,
            triggers.iter().map(|e| e.timestamp).collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_vector_mask_matches_scalar_expansion() {
        let mut words = vec![0x8001, 0x6123, 0x002a];
        let mut expected = Vec::new();
        for polarity in [0_u8, 1] {
            for (kind, width) in [(0x4000, 12), (0x5000, 8)] {
                for mask in 0..(1_u16 << width) {
                    words.push(0x3000 | ((polarity as u16) << 11) | 100);
                    // The high payload bits of VECT_8 are reserved, not events.
                    let reserved = if width == 8 { 0x0f00 } else { 0 };
                    words.push(kind | reserved | mask);
                    for bit in 0..width {
                        if mask & (1 << bit) != 0 {
                            expected.push(CdEvent::new(100 + bit, 42, polarity, 0x1123));
                        }
                    }
                    // Empty vectors must still advance the base coordinate.
                    words.extend([kind, 0x4001]);
                    expected.push(CdEvent::new(100 + 2 * width, 42, polarity, 0x1123));
                }
            }
        }

        let (rows, triggers) = decode_words(&words);
        assert_eq!(rows, expected);
        assert!(triggers.is_empty());
        let mut columns = ColumnarEventSink::default();
        Evt3Decoder::new().decode_buffer_into(&words, &mut columns);
        assert_columns_match(&columns, &expected, &[]);

        let bytes = words_to_bytes(&words);
        let mut decoder = Evt3Decoder::new();
        columns.clear();
        for chunk in bytes.chunks(4095) {
            decoder.decode_bytes_into(chunk, &mut columns);
        }
        decoder.finish_stream().unwrap();
        assert_columns_match(&columns, &expected, &[]);
    }

    #[test]
    fn vector_groups_and_interleaved_state_match_at_every_boundary() {
        let mut words = vec![0x8fff, 0x6ffe, 0x0003];
        let mut expected = Vec::new();
        let masks = [0, u32::MAX, 0xaaaa_aaaa, 0x5555_5555]
            .into_iter()
            .chain((0..32).map(|bit| 1_u32 << bit));
        for (index, mask) in masks.enumerate() {
            let polarity = (index % 2) as u8;
            words.extend([
                0x3014 | ((polarity as u16) << 11),
                0x4000 | (mask & 0x0fff) as u16,
                0x4000 | ((mask >> 12) & 0x0fff) as u16,
                0x5f00 | (mask >> 24) as u16, // Reserved VECT_8 bits must be ignored.
                0x4001,
            ]);
            for bit in 0..32 {
                if mask & (1 << bit) != 0 {
                    expected.push(CdEvent::new(20 + bit, 3, polarity, 0xfffffe));
                }
            }
            expected.push(CdEvent::new(52, 3, polarity, 0xfffffe));
        }

        // Partial groups and interleaved state words must update state in order.
        words.extend([
            0x380a, 0x4001, 0x6003, 0x4001, 0x4001, 0x0009, 0x5001, 0xa301, 0x8000, 0x2802,
        ]);
        expected.extend([
            CdEvent::new(10, 3, 1, 0xfffffe),
            CdEvent::new(22, 3, 1, 0xfff003),
            CdEvent::new(34, 3, 1, 0xfff003),
            CdEvent::new(46, 9, 1, 0xfff003),
            CdEvent::new(2, 9, 1, 0x1000000),
        ]);
        let triggers = [TriggerEvent::new(1, 3, 0xfff003)];
        assert_eq!(decode_words(&words), (expected.clone(), triggers.to_vec()));

        for chunk_size in [1, 2, 3, 4, 5, 32] {
            let mut decoder = Evt3Decoder::new();
            let mut columns = ColumnarEventSink::default();
            for chunk in words.chunks(chunk_size) {
                decoder.decode_buffer_into(chunk, &mut columns);
            }
            assert_columns_match(&columns, &expected, &triggers);
        }
        let bytes = words_to_bytes(&words);
        for split in 0..=bytes.len() {
            let mut decoder = Evt3Decoder::new();
            let mut columns = ColumnarEventSink::default();
            decoder.decode_bytes_into(&bytes[..split], &mut columns);
            decoder.decode_bytes_into(&bytes[split..], &mut columns);
            decoder.finish_stream().unwrap();
            assert_columns_match(&columns, &expected, &triggers);
        }
    }

    #[test]
    fn synchronization_and_rollover_survive_every_byte_split() {
        let words = [
            0x6032, 0x0042, 0x3870, 0x4fff, 0xa101, // Ignored before TIME_HIGH.
            0x8fff, 0x2001, // First TIME_HIGH, followed by x=1 at y=0.
            0x6ffe, 0x0002, 0x380a, 0x4005, 0xa301, // Two vector events and a trigger.
            0x8000, 0x6003, 0x2004, 0xa400, // Timestamp rollover.
        ];
        let bytes = words_to_bytes(&words);
        let expected = [
            CdEvent::new(1, 0, 0, 0xfff000),
            CdEvent::new(10, 2, 1, 0xfffffe),
            CdEvent::new(12, 2, 1, 0xfffffe),
            CdEvent::new(4, 2, 0, 0x1000003),
        ];
        let triggers = [
            TriggerEvent::new(1, 3, 0xfffffe),
            TriggerEvent::new(0, 4, 0x1000003),
        ];
        assert_eq!(decode_words(&words), (expected.to_vec(), triggers.to_vec()));

        for split in 0..=bytes.len() {
            let mut decoder = Evt3Decoder::new();
            let mut columns = ColumnarEventSink::default();
            decoder.decode_bytes_into(&bytes[..split], &mut columns);
            decoder.decode_bytes_into(&[], &mut columns);
            decoder.decode_bytes_into(&bytes[split..], &mut columns);
            decoder.finish_stream().unwrap();
            assert_columns_match(&columns, &expected, &triggers);
        }
    }

    #[test]
    fn synchronization_can_span_multiple_input_chunks() {
        let mut words = vec![0x2fff; 4097];
        words.extend([0x8002, 0x6003, 0x0004, 0x2805, 0xa601]);
        let bytes = words_to_bytes(&words);
        let expected = [CdEvent::new(5, 4, 1, 0x2003)];
        let triggers = [TriggerEvent::new(1, 6, 0x2003)];
        for chunk_size in [1, 2, 3, 4095, 8192, 8193] {
            let mut decoder = Evt3Decoder::new();
            let mut columns = ColumnarEventSink::default();
            for chunk in bytes.chunks(chunk_size) {
                decoder.decode_bytes_into(chunk, &mut columns);
            }
            decoder.finish_stream().unwrap();
            assert_columns_match(&columns, &expected, &triggers);
        }
        let mut decoder = Evt3Decoder::new();
        let mut columns = ColumnarEventSink::default();
        for chunk in words.chunks(4097) {
            decoder.decode_buffer_into(chunk, &mut columns);
        }
        assert_columns_match(&columns, &expected, &triggers);
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
