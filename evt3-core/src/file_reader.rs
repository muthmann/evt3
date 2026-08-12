//! Bounded-memory file decoding.

use crate::decoder::{DecodeError, Evt3Decoder};
use crate::sink::EventSink;
use crate::types::SensorMetadata;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// Default amount of encoded input read for one raw-file batch.
pub const DEFAULT_BATCH_BYTES: usize = 8 * 1024 * 1024;

/// Incremental decoder for raw and, when enabled, HDF5 event files.
///
/// Each call to [`Self::read_next_into`] processes a bounded input batch and
/// preserves decoder state for the next call.
pub struct EventFileReader {
    inner: EventFileReaderInner,
    metadata: SensorMetadata,
}

enum EventFileReaderInner {
    Raw {
        decoder: Evt3Decoder,
        reader: BufReader<File>,
        buffer: Vec<u8>,
        finished: bool,
    },
    #[cfg(feature = "hdf5")]
    Hdf5(crate::hdf5_decoder::Hdf5BatchReader),
}

impl EventFileReader {
    /// Opens an event file for bounded-memory decoding.
    pub fn open<P: AsRef<Path>>(path: P, batch_bytes: usize) -> Result<Self, DecodeError> {
        let path = path.as_ref();
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let batch_bytes = batch_bytes.max(2);

        #[cfg(feature = "hdf5")]
        if matches!(extension.as_str(), "h5" | "hdf5") {
            let reader = crate::hdf5_decoder::Hdf5BatchReader::open(path, batch_bytes)?;
            let metadata = reader.metadata().clone();
            return Ok(Self {
                inner: EventFileReaderInner::Hdf5(reader),
                metadata,
            });
        }

        #[cfg(not(feature = "hdf5"))]
        if matches!(extension.as_str(), "h5" | "hdf5") {
            return Err(DecodeError::InvalidFormat(
                "HDF5 input requires building with the 'hdf5' cargo feature".to_string(),
            ));
        }

        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut decoder = Evt3Decoder::new();
        decoder.parse_header(&mut reader)?;
        let metadata = decoder.metadata.clone();

        Ok(Self {
            inner: EventFileReaderInner::Raw {
                decoder,
                reader,
                buffer: vec![0; batch_bytes],
                finished: false,
            },
            metadata,
        })
    }

    /// Returns metadata parsed when the reader was opened.
    pub fn metadata(&self) -> &SensorMetadata {
        &self.metadata
    }

    /// Decodes the next bounded batch into `sink`.
    ///
    /// Returns `true` when a batch was processed and `false` after EOF.
    pub fn read_next_into<S: EventSink>(&mut self, sink: &mut S) -> Result<bool, DecodeError> {
        match &mut self.inner {
            EventFileReaderInner::Raw {
                decoder,
                reader,
                buffer,
                finished,
            } => {
                if *finished {
                    return Ok(false);
                }

                loop {
                    let bytes_read = reader.read(buffer)?;
                    if bytes_read == 0 {
                        decoder.discard_trailing_file_padding();
                        decoder.finish_stream()?;
                        *finished = true;
                        return Ok(false);
                    }

                    let before_cd = sink.cd_len();
                    let before_triggers = sink.trigger_len();
                    decoder.decode_bytes_into(&buffer[..bytes_read], sink);
                    if sink.cd_len() != before_cd || sink.trigger_len() != before_triggers {
                        return Ok(true);
                    }
                }
            }
            #[cfg(feature = "hdf5")]
            EventFileReaderInner::Hdf5(reader) => reader.read_next_into(sink),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColumnarEventSink;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn raw_reader_emits_bounded_batches_matching_decode_file() {
        let words = [0x8000_u16, 0x6001, 0x0002, 0x2803, 0x6002, 0x2004];
        let mut file = NamedTempFile::new().unwrap();
        for word in words {
            file.write_all(&word.to_le_bytes()).unwrap();
        }
        file.flush().unwrap();

        let expected = Evt3Decoder::new().decode_file(file.path()).unwrap();
        let mut reader = EventFileReader::open(file.path(), 5).unwrap();
        let mut sink = ColumnarEventSink::default();
        let mut events = Vec::new();

        while reader.read_next_into(&mut sink).unwrap() {
            events.extend((0..sink.cd.len()).map(|index| {
                crate::CdEvent::new(
                    sink.cd.x[index],
                    sink.cd.y[index],
                    sink.cd.polarity[index],
                    sink.cd.timestamp[index],
                )
            }));
            sink.clear();
        }

        assert_eq!(events, expected.cd_events);
    }
}
