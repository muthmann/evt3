use std::path::Path;

use hdf5::types::VarLenUnicode;
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

#[derive(H5Type, Clone, Debug)]
#[repr(C)]
struct RawTriggerEvent {
    p: i16,
    id: i16,
    t: i64,
}

pub(crate) fn decode_hdf5(
    decoder: &mut Evt3Decoder,
    path: &Path,
) -> Result<DecodeResult, DecodeError> {
    let file = File::open(path)?;

    decoder.metadata = parse_geometry(&file)?;

    let cd_events = read_cd_events(&file)?;
    let trigger_events = read_trigger_events(&file)?;

    if cd_events.is_empty() && trigger_events.is_empty() {
        return Err(DecodeError::MissingGroup(
            "neither CD/events nor EXT_TRIGGER/events dataset found".to_string(),
        ));
    }

    Ok(DecodeResult {
        cd_events,
        trigger_events,
        metadata: decoder.metadata.clone(),
    })
}

fn parse_geometry(file: &File) -> Result<SensorMetadata, DecodeError> {
    let attr = match file.attr("geometry") {
        Ok(attr) => attr,
        Err(_) => return Ok(SensorMetadata::default()),
    };

    let geometry = attr.read_scalar::<VarLenUnicode>()?;
    parse_geometry_value(geometry.as_str())
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
    let dataset = match open_events_dataset(file, "CD")? {
        Some(dataset) => dataset,
        None => return Ok(Vec::new()),
    };
    let raw_events = dataset.read_1d::<RawCdEvent>()?;
    let mut cd_events = Vec::with_capacity(raw_events.len());

    for raw in raw_events {
        cd_events.push(CdEvent::new(
            raw.x,
            raw.y,
            normalize_binary_flag(raw.p),
            decode_timestamp(raw.t)?,
        ));
    }

    Ok(cd_events)
}

fn read_trigger_events(file: &File) -> Result<Vec<TriggerEvent>, DecodeError> {
    let dataset = match open_events_dataset(file, "EXT_TRIGGER")? {
        Some(dataset) => dataset,
        None => return Ok(Vec::new()),
    };
    let raw_events = dataset.read_1d::<RawTriggerEvent>()?;
    let mut trigger_events = Vec::with_capacity(raw_events.len());

    for raw in raw_events {
        let id = u8::try_from(raw.id).map_err(|_| {
            DecodeError::InvalidFormat(format!("HDF5 trigger id must fit in u8, got {}", raw.id))
        })?;
        trigger_events.push(TriggerEvent::new(
            normalize_binary_flag(raw.p),
            id,
            decode_timestamp(raw.t)?,
        ));
    }

    Ok(trigger_events)
}

fn open_events_dataset(
    file: &File,
    group_name: &str,
) -> Result<Option<hdf5::Dataset>, DecodeError> {
    let group = match file.group(group_name) {
        Ok(group) => group,
        Err(_) => return Ok(None),
    };

    if !group
        .member_names()?
        .iter()
        .any(|member| member == "events")
    {
        return Ok(None);
    }

    Ok(Some(group.dataset("events")?))
}

fn normalize_binary_flag(flag: i16) -> u8 {
    u8::from(flag > 0)
}

fn decode_timestamp(timestamp: i64) -> Result<u64, DecodeError> {
    u64::try_from(timestamp).map_err(|_| {
        DecodeError::InvalidFormat(format!(
            "HDF5 event timestamp must be non-negative, got {}",
            timestamp
        ))
    })
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
