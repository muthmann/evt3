//! Python bindings for EVT 3.0 decoder with zero-copy numpy support.
//!
//! This module provides Python bindings using PyO3 that allow efficient
//! decoding of EVT 3.0 files with direct numpy array access to the decoded data.

use evt3::{
    ColumnarDecodeResult, ColumnarEventSink, EventColumns, EventFileReader, Evt3Decoder,
    TriggerColumns, DEFAULT_BATCH_BYTES,
};
use numpy::{IntoPyArray, PyArray1};
use pyo3::exceptions::PyIOError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::PathBuf;

/// Container for decoded CD events with zero-copy numpy access.
///
/// The data is stored in columnar format (separate arrays for x, y, p, t)
/// which is more efficient for numpy access and allows true zero-copy views.
#[pyclass]
pub struct Events {
    /// X coordinates
    x: Py<PyArray1<u16>>,
    /// Y coordinates
    y: Py<PyArray1<u16>>,
    /// Polarities
    polarity: Py<PyArray1<u8>>,
    /// Timestamps in microseconds
    timestamp: Py<PyArray1<u64>>,
    /// Event count
    event_count: usize,
    /// Sensor width
    sensor_width: u32,
    /// Sensor height
    sensor_height: u32,
}

#[pymethods]
impl Events {
    /// Returns the number of events.
    fn __len__(&self) -> usize {
        self.event_count
    }

    /// Returns a string representation.
    fn __repr__(&self) -> String {
        format!(
            "Events(count={}, sensor={}x{})",
            self.event_count, self.sensor_width, self.sensor_height
        )
    }

    /// Returns the X coordinates as a numpy array.
    ///
    /// Repeated access returns the same numpy array object.
    #[getter]
    fn x(&self, py: Python<'_>) -> Py<PyArray1<u16>> {
        self.x.clone_ref(py)
    }

    /// Returns the Y coordinates as a numpy array.
    #[getter]
    fn y(&self, py: Python<'_>) -> Py<PyArray1<u16>> {
        self.y.clone_ref(py)
    }

    /// Returns the polarities as a numpy array.
    ///
    /// Values: 0 = OFF (decrease in brightness), 1 = ON (increase)
    #[getter]
    fn polarity(&self, py: Python<'_>) -> Py<PyArray1<u8>> {
        self.polarity.clone_ref(py)
    }

    /// Alias for polarity (shorter name).
    #[getter]
    fn p(&self, py: Python<'_>) -> Py<PyArray1<u8>> {
        self.polarity.clone_ref(py)
    }

    /// Returns the timestamps as a numpy array (in microseconds).
    #[getter]
    fn timestamp(&self, py: Python<'_>) -> Py<PyArray1<u64>> {
        self.timestamp.clone_ref(py)
    }

    /// Alias for timestamp (shorter name).
    #[getter]
    fn t(&self, py: Python<'_>) -> Py<PyArray1<u64>> {
        self.timestamp.clone_ref(py)
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
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("x", self.x.clone_ref(py))?;
        dict.set_item("y", self.y.clone_ref(py))?;
        dict.set_item("polarity", self.polarity.clone_ref(py))?;
        dict.set_item("timestamp", self.timestamp.clone_ref(py))?;
        Ok(dict.unbind())
    }
}

impl Events {
    fn from_columns(py: Python<'_>, columns: EventColumns, width: u32, height: u32) -> Self {
        let len = columns.len();
        Self {
            x: columns.x.into_pyarray(py).unbind(),
            y: columns.y.into_pyarray(py).unbind(),
            polarity: columns.polarity.into_pyarray(py).unbind(),
            timestamp: columns.timestamp.into_pyarray(py).unbind(),
            event_count: len,
            sensor_width: width,
            sensor_height: height,
        }
    }
}

/// Container for decoded trigger events.
#[pyclass]
pub struct TriggerEvents {
    /// Trigger values (edge polarity): 0=falling, 1=rising
    value: Py<PyArray1<u8>>,
    /// Trigger channel IDs
    id: Py<PyArray1<u8>>,
    /// Timestamps in microseconds
    timestamp: Py<PyArray1<u64>>,
    event_count: usize,
}

#[pymethods]
impl TriggerEvents {
    /// Returns the number of trigger events.
    fn __len__(&self) -> usize {
        self.event_count
    }

    /// Returns the trigger values as a numpy array.
    #[getter]
    fn value(&self, py: Python<'_>) -> Py<PyArray1<u8>> {
        self.value.clone_ref(py)
    }

    /// Returns the trigger channel IDs as a numpy array.
    #[getter]
    fn id(&self, py: Python<'_>) -> Py<PyArray1<u8>> {
        self.id.clone_ref(py)
    }

    /// Returns the timestamps as a numpy array.
    #[getter]
    fn timestamp(&self, py: Python<'_>) -> Py<PyArray1<u64>> {
        self.timestamp.clone_ref(py)
    }
}

impl TriggerEvents {
    fn from_columns(py: Python<'_>, columns: TriggerColumns) -> Self {
        let event_count = columns.len();
        Self {
            value: columns.value.into_pyarray(py).unbind(),
            id: columns.id.into_pyarray(py).unbind(),
            timestamp: columns.timestamp.into_pyarray(py).unbind(),
            event_count,
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
    let result = py
        .detach(move || Evt3Decoder::new().decode_file_columns(&path))
        .map_err(decode_error)?;
    let events = events_from_result(py, result);

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

    let result = py
        .detach(move || Evt3Decoder::new().decode_file_columns(&path))
        .map_err(decode_error)?;
    let width = result.metadata.width;
    let height = result.metadata.height;
    let events = Events::from_columns(py, result.cd_events, width, height);
    let triggers = TriggerEvents::from_columns(py, result.trigger_events);

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
    let mut decoder = Evt3Decoder::new();
    decoder.metadata.width = sensor_width;
    decoder.metadata.height = sensor_height;

    let mut sink = ColumnarEventSink::default();
    decoder.decode_bytes_into(data, &mut sink);

    let events = Events::from_columns(py, sink.cd, sensor_width, sensor_height);
    Py::new(py, events)
}

/// Stateful incremental byte-stream decoder.
#[pyclass(name = "Decoder")]
pub struct PyDecoder {
    decoder: Evt3Decoder,
    sensor_width: u32,
    sensor_height: u32,
    finished: bool,
}

#[pymethods]
impl PyDecoder {
    #[new]
    #[pyo3(signature = (sensor_width=1280, sensor_height=720))]
    fn new(sensor_width: u32, sensor_height: u32) -> Self {
        let mut decoder = Evt3Decoder::new();
        decoder.metadata.width = sensor_width;
        decoder.metadata.height = sensor_height;
        Self {
            decoder,
            sensor_width,
            sensor_height,
            finished: false,
        }
    }

    /// Decodes one byte chunk and returns only the events from that chunk.
    fn feed(&mut self, py: Python<'_>, data: &[u8]) -> PyResult<Py<Events>> {
        if self.finished {
            return Err(PyIOError::new_err(
                "decoder is finished; call reset() before feeding more data",
            ));
        }
        let mut sink = ColumnarEventSink::default();
        self.decoder.decode_bytes_into(data, &mut sink);
        Py::new(
            py,
            Events::from_columns(py, sink.cd, self.sensor_width, self.sensor_height),
        )
    }

    /// Validates that the stream ended on a complete EVT3 word.
    fn finish(&mut self) -> PyResult<()> {
        self.decoder.finish_stream().map_err(decode_error)?;
        self.finished = true;
        Ok(())
    }

    /// Resets timestamp and byte-stream state for a new stream.
    fn reset(&mut self) {
        self.decoder.reset();
        self.finished = false;
    }

    #[getter]
    fn sensor_size(&self) -> (u32, u32) {
        (self.sensor_width, self.sensor_height)
    }
}

/// Iterator that decodes files in bounded-memory batches.
#[pyclass(name = "FileDecoder")]
pub struct PyFileDecoder {
    reader: EventFileReader,
    sensor_width: u32,
    sensor_height: u32,
}

#[pymethods]
impl PyFileDecoder {
    #[new]
    #[pyo3(signature = (path, batch_bytes=DEFAULT_BATCH_BYTES))]
    fn new(path: &str, batch_bytes: usize) -> PyResult<Self> {
        let reader = EventFileReader::open(path, batch_bytes).map_err(decode_error)?;
        let sensor_width = reader.metadata().width;
        let sensor_height = reader.metadata().height;
        Ok(Self {
            reader,
            sensor_width,
            sensor_height,
        })
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<Py<Events>>> {
        let mut sink = ColumnarEventSink::default();
        let has_batch = py
            .detach(|| self.reader.read_next_into(&mut sink))
            .map_err(decode_error)?;
        if !has_batch {
            return Ok(None);
        }
        Ok(Some(Py::new(
            py,
            Events::from_columns(py, sink.cd, self.sensor_width, self.sensor_height),
        )?))
    }

    #[getter]
    fn sensor_width(&self) -> u32 {
        self.sensor_width
    }

    #[getter]
    fn sensor_height(&self) -> u32 {
        self.sensor_height
    }

    #[getter]
    fn sensor_size(&self) -> (u32, u32) {
        (self.sensor_width, self.sensor_height)
    }
}

#[pyfunction]
#[pyo3(signature = (path, batch_bytes=DEFAULT_BATCH_BYTES))]
fn decode_file_batches(path: &str, batch_bytes: usize) -> PyResult<PyFileDecoder> {
    PyFileDecoder::new(path, batch_bytes)
}

/// Iterator that decodes bounded file batches including external triggers.
#[pyclass(name = "FileDecoderWithTriggers")]
pub struct PyFileDecoderWithTriggers {
    reader: EventFileReader,
    sensor_width: u32,
    sensor_height: u32,
}

#[pymethods]
impl PyFileDecoderWithTriggers {
    #[new]
    #[pyo3(signature = (path, batch_bytes=DEFAULT_BATCH_BYTES))]
    fn new(path: &str, batch_bytes: usize) -> PyResult<Self> {
        let reader = EventFileReader::open(path, batch_bytes).map_err(decode_error)?;
        let sensor_width = reader.metadata().width;
        let sensor_height = reader.metadata().height;
        Ok(Self {
            reader,
            sensor_width,
            sensor_height,
        })
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<(Py<Events>, Py<TriggerEvents>)>> {
        let mut sink = ColumnarEventSink::default();
        let has_batch = py
            .detach(|| self.reader.read_next_into(&mut sink))
            .map_err(decode_error)?;
        if !has_batch {
            return Ok(None);
        }

        let events = Events::from_columns(py, sink.cd, self.sensor_width, self.sensor_height);
        let triggers = TriggerEvents::from_columns(py, sink.triggers);
        Ok(Some((Py::new(py, events)?, Py::new(py, triggers)?)))
    }

    #[getter]
    fn sensor_width(&self) -> u32 {
        self.sensor_width
    }

    #[getter]
    fn sensor_height(&self) -> u32 {
        self.sensor_height
    }

    #[getter]
    fn sensor_size(&self) -> (u32, u32) {
        (self.sensor_width, self.sensor_height)
    }
}

#[pyfunction]
#[pyo3(signature = (path, batch_bytes=DEFAULT_BATCH_BYTES))]
fn decode_file_batches_with_triggers(
    path: &str,
    batch_bytes: usize,
) -> PyResult<PyFileDecoderWithTriggers> {
    PyFileDecoderWithTriggers::new(path, batch_bytes)
}

fn events_from_result(py: Python<'_>, result: ColumnarDecodeResult) -> Events {
    Events::from_columns(
        py,
        result.cd_events,
        result.metadata.width,
        result.metadata.height,
    )
}

fn decode_error(error: evt3::DecodeError) -> PyErr {
    PyIOError::new_err(format!("Failed to decode EVT3 data: {error}"))
}

/// EVT 3.0 decoder module for Python.
#[pymodule(gil_used = false)]
fn _evt3(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(decode_file, m)?)?;
    m.add_function(wrap_pyfunction!(decode_file_with_triggers, m)?)?;
    m.add_function(wrap_pyfunction!(decode_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(decode_file_batches, m)?)?;
    m.add_function(wrap_pyfunction!(decode_file_batches_with_triggers, m)?)?;
    m.add_class::<Events>()?;
    m.add_class::<TriggerEvents>()?;
    m.add_class::<PyDecoder>()?;
    m.add_class::<PyFileDecoder>()?;
    m.add_class::<PyFileDecoderWithTriggers>()?;
    Ok(())
}
