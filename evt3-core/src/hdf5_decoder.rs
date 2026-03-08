use std::path::Path;

use hdf5::types::{VarLenAscii, VarLenUnicode};
use hdf5::{File, H5Type};

use crate::decoder::{DecodeError, Evt3Decoder};
use crate::types::{CdEvent, DecodeResult, SensorMetadata, TriggerEvent};

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

pub(crate) fn decode_hdf5(
    decoder: &mut Evt3Decoder,
    path: &Path,
) -> Result<DecodeResult, DecodeError> {
    let file = File::open(path)?;

    let metadata = parse_geometry(&file)?;
    let cd_events = read_cd_events(&file)?;
    let trigger_events = read_trigger_events(&file)?;

    if cd_events.is_empty() && trigger_events.is_empty() {
        return Err(DecodeError::MissingGroup(
            "neither CD/events nor EXT_TRIGGER/events dataset found".to_string(),
        ));
    }

    decoder.metadata = metadata.clone();
    Ok(DecodeResult {
        cd_events,
        trigger_events,
        metadata,
    })
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

fn read_cd_events(file: &File) -> Result<Vec<CdEvent>, DecodeError> {
    let Some(dataset) = open_events_dataset(file, "CD")? else {
        return Ok(Vec::new());
    };
    dataset
        .read_1d::<RawCdEvent>()?
        .into_iter()
        .map(|raw| {
            Ok(CdEvent::new(
                raw.x,
                raw.y,
                normalize_binary_flag(raw.p),
                decode_timestamp(raw.t)?,
            ))
        })
        .collect()
}

fn read_trigger_events(file: &File) -> Result<Vec<TriggerEvent>, DecodeError> {
    let Some(dataset) = open_events_dataset(file, "EXT_TRIGGER")? else {
        return Ok(Vec::new());
    };
    dataset
        .read_1d::<RawTriggerEvent>()?
        .into_iter()
        .map(|raw| {
            let id = u8::try_from(raw.id).map_err(|_| {
                DecodeError::InvalidFormat(format!(
                    "HDF5 trigger id must fit in u8, got {}",
                    raw.id
                ))
            })?;
            Ok(TriggerEvent::new(
                normalize_binary_flag(raw.p),
                id,
                decode_timestamp(raw.t)?,
            ))
        })
        .collect()
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
