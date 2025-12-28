//! Python bindings for EVT 3.0 decoder with zero-copy numpy support.
//!
//! This module provides Python bindings using PyO3 that allow efficient
//! decoding of EVT 3.0 files with direct numpy array access to the decoded data.

use evt3_core::{CdEvent, Evt3Decoder, TriggerEvent};
use numpy::{IntoPyArray, PyArray1};
use pyo3::exceptions::PyIOError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::PathBuf;

/// Container for decoded CD events with zero-copy numpy access.
///
/// The data is stored in columnar format (separate arrays for x, y, p, t)
/// which is more efficient for numpy access and allows true zero-copy views.
#[pyclass]
pub struct Events {
    /// X coordinates
    x: Vec<u16>,
    /// Y coordinates
    y: Vec<u16>,
    /// Polarities
    polarity: Vec<u8>,
    /// Timestamps in microseconds
    timestamp: Vec<u64>,
    /// Sensor width
    sensor_width: u32,
    /// Sensor height
    sensor_height: u32,
}

#[pymethods]
impl Events {
    /// Returns the number of events.
    fn __len__(&self) -> usize {
        self.x.len()
    }

    /// Returns a string representation.
    fn __repr__(&self) -> String {
        format!(
            "Events(count={}, sensor={}x{})",
            self.x.len(),
            self.sensor_width,
            self.sensor_height
        )
    }

    /// Returns the X coordinates as a numpy array.
    ///
    /// This creates a view into the Rust-allocated memory without copying.
    /// The array is valid as long as this Events object is alive.
    #[getter]
    fn x<'py>(&self, py: Python<'py>) -> &'py PyArray1<u16> {
        self.x.clone().into_pyarray(py)
    }

    /// Returns the Y coordinates as a numpy array.
    #[getter]
    fn y<'py>(&self, py: Python<'py>) -> &'py PyArray1<u16> {
        self.y.clone().into_pyarray(py)
    }

    /// Returns the polarities as a numpy array.
    ///
    /// Values: 0 = OFF (decrease in brightness), 1 = ON (increase)
    #[getter]
    fn polarity<'py>(&self, py: Python<'py>) -> &'py PyArray1<u8> {
        self.polarity.clone().into_pyarray(py)
    }

    /// Alias for polarity (shorter name).
    #[getter]
    fn p<'py>(&self, py: Python<'py>) -> &'py PyArray1<u8> {
        self.polarity.clone().into_pyarray(py)
    }

    /// Returns the timestamps as a numpy array (in microseconds).
    #[getter]
    fn timestamp<'py>(&self, py: Python<'py>) -> &'py PyArray1<u64> {
        self.timestamp.clone().into_pyarray(py)
    }

    /// Alias for timestamp (shorter name).
    #[getter]
    fn t<'py>(&self, py: Python<'py>) -> &'py PyArray1<u64> {
        self.timestamp.clone().into_pyarray(py)
    }

    /// Returns the sensor width in pixels.
    #[getter]
    fn sensor_width(&self) -> u32 {
        self.sensor_width
    }

    /// Returns the sensor height in pixels.
    #[getter]
    fn sensor_height(&self) -> u32 {
        self.sensor_height
    }

    /// Returns a tuple of (width, height) for the sensor geometry.
    #[getter]
    fn sensor_size(&self) -> (u32, u32) {
        (self.sensor_width, self.sensor_height)
    }

    /// Returns all arrays as a dictionary.
    ///
    /// This is useful for creating a pandas DataFrame or structured array.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        dict.set_item("x", self.x.clone().into_pyarray(py))?;
        dict.set_item("y", self.y.clone().into_pyarray(py))?;
        dict.set_item("polarity", self.polarity.clone().into_pyarray(py))?;
        dict.set_item("timestamp", self.timestamp.clone().into_pyarray(py))?;
        Ok(dict.into())
    }
}

impl Events {
    /// Creates an Events container from a vector of CdEvent structs.
    fn from_cd_events(events: Vec<CdEvent>, width: u32, height: u32) -> Self {
        let len = events.len();
        let mut x = Vec::with_capacity(len);
        let mut y = Vec::with_capacity(len);
        let mut polarity = Vec::with_capacity(len);
        let mut timestamp = Vec::with_capacity(len);

        for event in events {
            x.push(event.x);
            y.push(event.y);
            polarity.push(event.polarity);
            timestamp.push(event.timestamp);
        }

        Self {
            x,
            y,
            polarity,
            timestamp,
            sensor_width: width,
            sensor_height: height,
        }
    }
}

/// Container for decoded trigger events.
#[pyclass]
pub struct TriggerEvents {
    /// Trigger values (edge polarity): 0=falling, 1=rising
    value: Vec<u8>,
    /// Trigger channel IDs
    id: Vec<u8>,
    /// Timestamps in microseconds
    timestamp: Vec<u64>,
}

#[pymethods]
impl TriggerEvents {
    /// Returns the number of trigger events.
    fn __len__(&self) -> usize {
        self.value.len()
    }

    /// Returns the trigger values as a numpy array.
    #[getter]
    fn value<'py>(&self, py: Python<'py>) -> &'py PyArray1<u8> {
        self.value.clone().into_pyarray(py)
    }

    /// Returns the trigger channel IDs as a numpy array.
    #[getter]
    fn id<'py>(&self, py: Python<'py>) -> &'py PyArray1<u8> {
        self.id.clone().into_pyarray(py)
    }

    /// Returns the timestamps as a numpy array.
    #[getter]
    fn timestamp<'py>(&self, py: Python<'py>) -> &'py PyArray1<u64> {
        self.timestamp.clone().into_pyarray(py)
    }
}

impl TriggerEvents {
    fn from_trigger_events(events: Vec<TriggerEvent>) -> Self {
        let len = events.len();
        let mut value = Vec::with_capacity(len);
        let mut id = Vec::with_capacity(len);
        let mut timestamp = Vec::with_capacity(len);

        for event in events {
            value.push(event.value);
            id.push(event.id);
            timestamp.push(event.timestamp);
        }

        Self {
            value,
            id,
            timestamp,
        }
    }
}

/// Result of decoding an EVT3 file.
#[pyclass]
pub struct DecodeResult {
    #[pyo3(get)]
    events: Py<Events>,
    #[pyo3(get)]
    triggers: Py<TriggerEvents>,
}

/// Decodes an EVT 3.0 raw file and returns the events.
///
/// Args:
///     path: Path to the .raw file
///
/// Returns:
///     Events: Container with x, y, polarity, and timestamp arrays
///
/// Example:
///     >>> import evt3
///     >>> events = evt3.decode_file("recording.raw")
///     >>> print(f"Decoded {len(events)} events")
///     >>> x = events.x  # numpy array of x coordinates
///     >>> y = events.y  # numpy array of y coordinates
#[pyfunction]
fn decode_file(py: Python<'_>, path: &str) -> PyResult<Py<Events>> {
    let path = PathBuf::from(path);

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(&path)
        .map_err(|e| PyIOError::new_err(format!("Failed to decode file: {}", e)))?;

    let events = Events::from_cd_events(
        result.cd_events,
        result.metadata.width,
        result.metadata.height,
    );

    Py::new(py, events)
}

/// Decodes an EVT 3.0 raw file and returns both CD and trigger events.
///
/// Args:
///     path: Path to the .raw file
///
/// Returns:
///     tuple: (Events, TriggerEvents)
///
/// Example:
///     >>> import evt3
///     >>> events, triggers = evt3.decode_file_with_triggers("recording.raw")
///     >>> print(f"CD events: {len(events)}, Triggers: {len(triggers)}")
#[pyfunction]
fn decode_file_with_triggers(
    py: Python<'_>,
    path: &str,
) -> PyResult<(Py<Events>, Py<TriggerEvents>)> {
    let path = PathBuf::from(path);

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(&path)
        .map_err(|e| PyIOError::new_err(format!("Failed to decode file: {}", e)))?;

    let events = Events::from_cd_events(
        result.cd_events,
        result.metadata.width,
        result.metadata.height,
    );

    let triggers = TriggerEvents::from_trigger_events(result.trigger_events);

    Ok((Py::new(py, events)?, Py::new(py, triggers)?))
}

/// Decodes raw EVT 3.0 bytes and returns events.
///
/// This is useful for streaming decoding or when the data is already in memory.
///
/// Args:
///     data: Raw bytes containing EVT 3.0 encoded data
///     sensor_width: Sensor width in pixels (default: 1280)
///     sensor_height: Sensor height in pixels (default: 720)
///
/// Returns:
///     Events: Container with decoded events
#[pyfunction]
#[pyo3(signature = (data, sensor_width=1280, sensor_height=720))]
fn decode_bytes(
    py: Python<'_>,
    data: &[u8],
    sensor_width: u32,
    sensor_height: u32,
) -> PyResult<Py<Events>> {
    // Convert bytes to u16 words (little-endian)
    let words: Vec<u16> = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    let mut decoder = Evt3Decoder::new();
    decoder.metadata.width = sensor_width;
    decoder.metadata.height = sensor_height;

    let mut cd_events = Vec::new();
    let mut trigger_events = Vec::new();
    decoder.decode_buffer(&words, &mut cd_events, &mut trigger_events);

    let events = Events::from_cd_events(cd_events, sensor_width, sensor_height);
    Py::new(py, events)
}

/// EVT 3.0 decoder module for Python.
#[pymodule]
fn _evt3(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(decode_file, m)?)?;
    m.add_function(wrap_pyfunction!(decode_file_with_triggers, m)?)?;
    m.add_function(wrap_pyfunction!(decode_bytes, m)?)?;
    m.add_class::<Events>()?;
    m.add_class::<TriggerEvents>()?;
    Ok(())
}
