//! Output format writers for decoded EVT 3.0 data.
//!
//! Supports multiple output formats including CSV, binary, and Apache Arrow IPC.

use crate::types::{CdEvent, SensorMetadata, TriggerEvent};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during output writing.
#[derive(Error, Debug)]
pub enum OutputError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid format: {0}")]
    InvalidFormat(String),
}

/// Field ordering for output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldOrder {
    /// x, y, p, t (default, matches C++ reference)
    #[default]
    XYPT,
    /// t, x, y, p
    TXYP,
    /// x, y, t, p
    XYTP,
    /// Custom order specified by indices
    Custom([usize; 4]),
}

impl std::str::FromStr for FieldOrder {
    type Err = OutputError;

    /// Parses a field order from a format string like "x,y,p,t" or "t,x,y,p".
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<String> = s.split(',').map(|p| p.trim().to_lowercase()).collect();

        if parts.len() != 4 {
            return Err(OutputError::InvalidFormat(
                "Format must have exactly 4 fields: x, y, p, t".to_string(),
            ));
        }

        // Map field names to indices: x=0, y=1, p=2, t=3
        let mut indices = [0usize; 4];
        let mut used = [false; 4];

        for (i, part) in parts.iter().enumerate() {
            let field_idx = match part.as_str() {
                "x" => 0,
                "y" => 1,
                "p" | "pol" | "polarity" => 2,
                "t" | "time" | "timestamp" => 3,
                _ => {
                    return Err(OutputError::InvalidFormat(format!(
                        "Unknown field: {}. Use x, y, p, t",
                        part
                    )))
                }
            };

            if used[field_idx] {
                return Err(OutputError::InvalidFormat(format!(
                    "Duplicate field: {}",
                    part
                )));
            }

            indices[i] = field_idx;
            used[field_idx] = true;
        }

        // Check for common patterns
        if indices == [0, 1, 2, 3] {
            Ok(Self::XYPT)
        } else if indices == [3, 0, 1, 2] {
            Ok(Self::TXYP)
        } else if indices == [0, 1, 3, 2] {
            Ok(Self::XYTP)
        } else {
            Ok(Self::Custom(indices))
        }
    }
}

impl FieldOrder {
    /// Returns the header string for this field order.
    pub fn header(&self) -> &'static str {
        match self {
            Self::XYPT => "x,y,polarity,timestamp",
            Self::TXYP => "timestamp,x,y,polarity",
            Self::XYTP => "x,y,timestamp,polarity",
            Self::Custom(_) => "x,y,polarity,timestamp", // Will be reordered when writing
        }
    }
}

/// CSV output writer for CD events.
pub struct CsvWriter<W: Write> {
    writer: BufWriter<W>,
    field_order: FieldOrder,
}

impl<W: Write> CsvWriter<W> {
    /// Creates a new CSV writer.
    pub fn new(writer: W, field_order: FieldOrder) -> Self {
        Self {
            writer: BufWriter::new(writer),
            field_order,
        }
    }

    /// Writes the CSV header with optional geometry metadata.
    pub fn write_header(&mut self, metadata: Option<&SensorMetadata>) -> Result<(), OutputError> {
        // Write geometry header if available
        if let Some(meta) = metadata {
            writeln!(self.writer, "%geometry:{},{}", meta.width, meta.height)?;
        }
        Ok(())
    }

    /// Writes a batch of CD events.
    pub fn write_events(&mut self, events: &[CdEvent]) -> Result<(), OutputError> {
        for event in events {
            self.write_event(event)?;
        }
        Ok(())
    }

    /// Writes a single CD event.
    #[inline]
    fn write_event(&mut self, event: &CdEvent) -> Result<(), OutputError> {
        match self.field_order {
            FieldOrder::XYPT => {
                writeln!(
                    self.writer,
                    "{},{},{},{}",
                    event.x, event.y, event.polarity, event.timestamp
                )?;
            }
            FieldOrder::TXYP => {
                writeln!(
                    self.writer,
                    "{},{},{},{}",
                    event.timestamp, event.x, event.y, event.polarity
                )?;
            }
            FieldOrder::XYTP => {
                writeln!(
                    self.writer,
                    "{},{},{},{}",
                    event.x, event.y, event.timestamp, event.polarity
                )?;
            }
            FieldOrder::Custom(indices) => {
                let values = [
                    event.x as u64,
                    event.y as u64,
                    event.polarity as u64,
                    event.timestamp,
                ];
                writeln!(
                    self.writer,
                    "{},{},{},{}",
                    values[indices[0]], values[indices[1]], values[indices[2]], values[indices[3]]
                )?;
            }
        }
        Ok(())
    }

    /// Flushes the writer.
    pub fn flush(&mut self) -> Result<(), OutputError> {
        self.writer.flush()?;
        Ok(())
    }
}

/// CSV writer for trigger events.
pub struct TriggerCsvWriter<W: Write> {
    writer: BufWriter<W>,
}

impl<W: Write> TriggerCsvWriter<W> {
    /// Creates a new trigger CSV writer.
    pub fn new(writer: W) -> Self {
        Self {
            writer: BufWriter::new(writer),
        }
    }

    /// Writes a batch of trigger events.
    pub fn write_events(&mut self, events: &[TriggerEvent]) -> Result<(), OutputError> {
        for event in events {
            writeln!(
                self.writer,
                "{},{},{}",
                event.value, event.id, event.timestamp
            )?;
        }
        Ok(())
    }

    /// Flushes the writer.
    pub fn flush(&mut self) -> Result<(), OutputError> {
        self.writer.flush()?;
        Ok(())
    }
}

/// Binary output format for CD events.
///
/// Each event is written as a packed struct:
/// - x: u16 (2 bytes)
/// - y: u16 (2 bytes)  
/// - polarity: u8 (1 byte)
/// - padding: u8 (1 byte, for alignment)
/// - timestamp: u64 (8 bytes)
///   Total: 14 bytes per event (padded to 16 for alignment)
pub struct BinaryWriter<W: Write> {
    writer: BufWriter<W>,
}

impl<W: Write> BinaryWriter<W> {
    /// Creates a new binary writer.
    pub fn new(writer: W) -> Self {
        Self {
            writer: BufWriter::new(writer),
        }
    }

    /// Writes a header with metadata.
    pub fn write_header(
        &mut self,
        metadata: &SensorMetadata,
        event_count: u64,
    ) -> Result<(), OutputError> {
        // Magic number "EVT3BIN\0"
        self.writer.write_all(b"EVT3BIN\0")?;
        // Version (u32)
        self.writer.write_all(&1u32.to_le_bytes())?;
        // Sensor width (u32)
        self.writer.write_all(&metadata.width.to_le_bytes())?;
        // Sensor height (u32)
        self.writer.write_all(&metadata.height.to_le_bytes())?;
        // Event count (u64)
        self.writer.write_all(&event_count.to_le_bytes())?;
        Ok(())
    }

    /// Writes a batch of CD events.
    pub fn write_events(&mut self, events: &[CdEvent]) -> Result<(), OutputError> {
        for event in events {
            self.writer.write_all(&event.x.to_le_bytes())?;
            self.writer.write_all(&event.y.to_le_bytes())?;
            self.writer.write_all(&[event.polarity, 0])?; // polarity + padding
            self.writer.write_all(&event.timestamp.to_le_bytes())?;
        }
        Ok(())
    }

    /// Flushes the writer.
    pub fn flush(&mut self) -> Result<(), OutputError> {
        self.writer.flush()?;
        Ok(())
    }
}

/// Writes CD events to a CSV file.
pub fn write_csv<P: AsRef<Path>>(
    path: P,
    events: &[CdEvent],
    metadata: Option<&SensorMetadata>,
    field_order: FieldOrder,
) -> Result<(), OutputError> {
    let file = File::create(path)?;
    let mut writer = CsvWriter::new(file, field_order);
    writer.write_header(metadata)?;
    writer.write_events(events)?;
    writer.flush()?;
    Ok(())
}

/// Writes trigger events to a CSV file.
pub fn write_trigger_csv<P: AsRef<Path>>(
    path: P,
    events: &[TriggerEvent],
) -> Result<(), OutputError> {
    let file = File::create(path)?;
    let mut writer = TriggerCsvWriter::new(file);
    writer.write_events(events)?;
    writer.flush()?;
    Ok(())
}

/// Writes CD events to a binary file.
pub fn write_binary<P: AsRef<Path>>(
    path: P,
    events: &[CdEvent],
    metadata: &SensorMetadata,
) -> Result<(), OutputError> {
    let file = File::create(path)?;
    let mut writer = BinaryWriter::new(file);
    writer.write_header(metadata, events.len() as u64)?;
    writer.write_events(events)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_field_order_parsing() {
        assert_eq!(FieldOrder::from_str("x,y,p,t").unwrap(), FieldOrder::XYPT);
        assert_eq!(FieldOrder::from_str("t,x,y,p").unwrap(), FieldOrder::TXYP);
        assert_eq!(FieldOrder::from_str("x,y,t,p").unwrap(), FieldOrder::XYTP);
        assert_eq!(
            FieldOrder::from_str("X, Y, P, T").unwrap(),
            FieldOrder::XYPT
        );
    }

    #[test]
    fn test_field_order_invalid() {
        assert!(FieldOrder::from_str("x,y,z,t").is_err());
        assert!(FieldOrder::from_str("x,y,p").is_err());
        assert!(FieldOrder::from_str("x,x,y,t").is_err());
    }

    #[test]
    fn test_csv_writer() {
        let mut output = Vec::new();
        {
            let mut writer = CsvWriter::new(&mut output, FieldOrder::XYPT);
            writer
                .write_header(Some(&SensorMetadata {
                    width: 640,
                    height: 480,
                }))
                .unwrap();
            writer
                .write_events(&[
                    CdEvent::new(100, 200, 1, 12345),
                    CdEvent::new(101, 201, 0, 12346),
                ])
                .unwrap();
            writer.flush().unwrap();
        }

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("%geometry:640,480"));
        assert!(output_str.contains("100,200,1,12345"));
        assert!(output_str.contains("101,201,0,12346"));
    }

    #[test]
    fn test_csv_writer_txyp_order() {
        let mut output = Vec::new();
        {
            let mut writer = CsvWriter::new(&mut output, FieldOrder::TXYP);
            writer
                .write_events(&[CdEvent::new(100, 200, 1, 12345)])
                .unwrap();
            writer.flush().unwrap();
        }

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("12345,100,200,1"));
    }
}
