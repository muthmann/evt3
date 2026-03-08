//! EVT 3.0 decoder library for Prophesee event cameras.
//!
//! This crate provides a high-performance decoder for the EVT 3.0 raw data format
//! used by Prophesee event cameras. It supports decoding CD (Change Detection) events
//! and external trigger events.
//!
//! # File decode
//!
//! ```no_run
//! use evt3_core::Evt3Decoder;
//!
//! let mut decoder = Evt3Decoder::new();
//! let result = decoder.decode_file("recording.raw").unwrap();
//!
//! println!("Decoded {} CD events", result.cd_events.len());
//! println!("Sensor: {}x{}", result.metadata.width, result.metadata.height);
//! ```
//!
//! # Streaming bytes
//!
//! ```no_run
//! use evt3_core::Evt3Decoder;
//!
//! let mut decoder = Evt3Decoder::new();
//! let mut cd_events = Vec::new();
//! let mut trigger_events = Vec::new();
//!
//! let chunk1: &[u8] = &[];
//! let chunk2: &[u8] = &[];
//!
//! decoder
//!     .decode_bytes(chunk1, &mut cd_events, &mut trigger_events)
//!     .unwrap();
//! decoder
//!     .decode_bytes(chunk2, &mut cd_events, &mut trigger_events)
//!     .unwrap();
//! decoder.finish_stream().unwrap();
//! ```
//!
//! # Features
//!
//! - Full EVT 3.0 specification support including vectorized events
//! - File header parsing for sensor metadata
//! - Multiple output formats (CSV, binary, Arrow IPC)
//! - Customizable field ordering for output
//! - Incremental byte-stream decoding for live camera pipelines

pub mod decoder;
#[cfg(feature = "hdf5")]
pub(crate) mod hdf5_decoder;
pub mod output;
pub mod parser;
pub mod types;

// Re-export commonly used types
pub use decoder::{DecodeError, Evt3Decoder};
pub use output::{FieldOrder, OutputError};
pub use types::{CdEvent, DecodeResult, SensorMetadata, TriggerEvent};
