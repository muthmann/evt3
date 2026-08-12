use std::path::Path;

use hdf5::types::{VarLenAscii, VarLenUnicode};
use hdf5::{File, H5Type};

use crate::decoder::{DecodeError, Evt3Decoder};
use crate::sink::EventSink;
use crate::types::SensorMetadata;

const HDF5_BATCH_EVENTS: usize = 1_000_000;

#[derive(H5Type, Clone, Debug)]
#[repr(C)]
struct RawCdEvent {
    x: u16,
    y: u16,
    p: i16,
    t: i64,
}

// Prophesee HDF5 trigger event layout (verified via h5py dtype inspection):
// field order is p, t, id (not p, id, t).
// #[repr(C)] on {i16, i64, i16} naturally produces the matching 24-byte layout:
// p @ 0 (2 bytes), implicit 6-byte pad, t @ 8 (8 bytes), id @ 16 (2 bytes),
// implicit 6-byte tail pad to align the struct to 8 bytes, itemsize = 24.
#[derive(H5Type, Clone, Debug)]
#[repr(C)]
struct RawTriggerEvent {
    p: i16,
    t: i64,
    id: i16,
}

pub(crate) fn decode_hdf5_into<S: EventSink>(
    decoder: &mut Evt3Decoder,
    path: &Path,
    sink: &mut S,
) -> Result<(), DecodeError> {
    let mut reader = Hdf5BatchReader::open(path, HDF5_BATCH_EVENTS * 16)?;
    sink.reserve_cd(reader.cd_len);
    sink.reserve_triggers(reader.trigger_len);
    while reader.read_next_into(sink)? {}
    decoder.metadata = reader.metadata;
    Ok(())
}

pub(crate) struct Hdf5BatchReader {
    _file: File,
    metadata: SensorMetadata,
    cd_dataset: Option<hdf5::Dataset>,
    trigger_dataset: Option<hdf5::Dataset>,
    cd_offset: usize,
    trigger_offset: usize,
    cd_len: usize,
    trigger_len: usize,
    batch_events: usize,
}

impl Hdf5BatchReader {
    pub(crate) fn open(path: &Path, batch_bytes: usize) -> Result<Self, DecodeError> {
        let file = File::open(path)?;
        let metadata = parse_geometry(&file)?;
        let cd_dataset = open_events_dataset(&file, "CD")?;
        let trigger_dataset = open_events_dataset(&file, "EXT_TRIGGER")?;
        let cd_len = cd_dataset.as_ref().map_or(0, |dataset| dataset.size());
        let trigger_len = trigger_dataset.as_ref().map_or(0, |dataset| dataset.size());

        if cd_len == 0 && trigger_len == 0 {
            return Err(DecodeError::MissingGroup(
                "neither CD/events nor EXT_TRIGGER/events dataset found".to_string(),
            ));
        }

        Ok(Self {
            _file: file,
            metadata,
            cd_dataset,
            trigger_dataset,
            cd_offset: 0,
            trigger_offset: 0,
            cd_len,
            trigger_len,
            batch_events: (batch_bytes / std::mem::size_of::<RawCdEvent>()).max(1),
        })
    }

    pub(crate) fn metadata(&self) -> &SensorMetadata {
        &self.metadata
    }

    pub(crate) fn read_next_into<S: EventSink>(
        &mut self,
        sink: &mut S,
    ) -> Result<bool, DecodeError> {
        if self.cd_offset >= self.cd_len && self.trigger_offset >= self.trigger_len {
            return Ok(false);
        }

        if let Some(dataset) = &self.cd_dataset {
            let end = (self.cd_offset + self.batch_events).min(self.cd_len);
            if self.cd_offset < end {
                sink.reserve_cd(end - self.cd_offset);
                for raw in dataset.read_slice_1d::<RawCdEvent, _>(self.cd_offset..end)? {
                    sink.push_cd(
                        raw.x,
                        raw.y,
                        normalize_binary_flag(raw.p),
                        decode_timestamp(raw.t)?,
                    );
                }
                self.cd_offset = end;
            }
        }

        if let Some(dataset) = &self.trigger_dataset {
            let end = (self.trigger_offset + self.batch_events).min(self.trigger_len);
            if self.trigger_offset < end {
                sink.reserve_triggers(end - self.trigger_offset);
                for raw in dataset.read_slice_1d::<RawTriggerEvent, _>(self.trigger_offset..end)? {
                    let id = u8::try_from(raw.id).map_err(|_| {
                        DecodeError::InvalidFormat(format!(
                            "HDF5 trigger id must fit in u8, got {}",
                            raw.id
                        ))
                    })?;
                    sink.push_trigger(normalize_binary_flag(raw.p), id, decode_timestamp(raw.t)?);
                }
                self.trigger_offset = end;
            }
        }

        Ok(true)
    }
}

fn parse_geometry(file: &File) -> Result<SensorMetadata, DecodeError> {
    let attr = match file.attr("geometry") {
        Ok(attr) => attr,
        Err(_) => return Ok(SensorMetadata::default()),
    };

    // Prophesee files store geometry as ASCII; try that first and fall back to
    // Unicode for files (e.g. synthetic test fixtures) that use UTF-8 strings.
    let geometry = attr
        .read_scalar::<VarLenAscii>()
        .map(|s| s.as_str().to_string())
        .or_else(|_| attr.read_scalar::<VarLenUnicode>().map(|s| s.to_string()))?;
    parse_geometry_value(&geometry)
}

fn parse_geometry_value(geometry: &str) -> Result<SensorMetadata, DecodeError> {
    let Some((width, height)) = geometry.split_once('x') else {
        return Err(DecodeError::MalformedGeometry(geometry.to_string()));
    };

    let width = width
        .parse()
        .map_err(|_| DecodeError::MalformedGeometry(geometry.to_string()))?;
    let height = height
        .parse()
        .map_err(|_| DecodeError::MalformedGeometry(geometry.to_string()))?;

    Ok(SensorMetadata { width, height })
}

fn open_events_dataset(
    file: &File,
    group_name: &str,
) -> Result<Option<hdf5::Dataset>, DecodeError> {
    let group = match file.group(group_name) {
        Ok(group) => group,
        Err(_) => return Ok(None),
    };
    // dataset() returns Err if "events" is absent; map that to None.
    Ok(group.dataset("events").ok())
}

#[inline(always)]
fn normalize_binary_flag(flag: i16) -> u8 {
    u8::from(flag > 0)
}

#[inline]
fn decode_timestamp(timestamp: i64) -> Result<u64, DecodeError> {
    if timestamp < 0 {
        return Err(DecodeError::InvalidFormat(format!(
            "HDF5 event timestamp must be non-negative, got {timestamp}"
        )));
    }
    Ok(timestamp as u64)
}

#[cfg(test)]
mod tests {
    use super::parse_geometry_value;
    use crate::decoder::DecodeError;

    #[test]
    fn parse_geometry_value_accepts_wxh() {
        let metadata = parse_geometry_value("640x480").unwrap();
        assert_eq!(metadata.width, 640);
        assert_eq!(metadata.height, 480);
    }

    #[test]
    fn parse_geometry_value_rejects_invalid_geometry() {
        let err = parse_geometry_value("640-480").unwrap_err();
        assert!(matches!(err, DecodeError::MalformedGeometry(_)));
    }
}
