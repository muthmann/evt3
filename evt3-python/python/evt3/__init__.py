"""
EVT3 - High-performance EVT 3.0 decoder for Prophesee event cameras.

This module provides a Rust-backed decoder for EVT 3.0 raw data files
with zero-copy numpy array support for efficient data handling.

All decoding entry points accept keyword-only ``threads``: 0 (default) selects
Auto, 1 disables worker threads, and n > 1 limits total decoding threads,
including the calling thread. Auto uses up to four threads. ``PARALLEL_ENABLED``
reports whether this build includes the optional parallel engine. HDF5 does not
use this EVT3 engine. Use threads=1 when an application already parallelizes files.

Example:
    >>> import evt3
    >>> events = evt3.decode_file("recording.raw")
    >>> print(f"Decoded {len(events)} events from {events.sensor_width}x{events.sensor_height} sensor")
    
    # Access data as numpy arrays
    >>> x = events.x  # np.ndarray[np.uint16]
    >>> y = events.y  # np.ndarray[np.uint16]
    >>> p = events.polarity  # np.ndarray[np.uint8]
    >>> t = events.timestamp  # np.ndarray[np.uint64]
    
    # Or get as dictionary for DataFrame creation
    >>> import pandas as pd
    >>> df = pd.DataFrame(events.to_dict())
"""

from importlib.metadata import PackageNotFoundError, version as _package_version

from . import augur
from ._evt3 import (
    Decoder as _Decoder,
    PARALLEL_ENABLED,
    decode_bytes as _decode_bytes,
    decode_file_batches as _decode_file_batches,
    decode_file_batches_with_triggers as _decode_file_batches_with_triggers,
    decode_file as _decode_file,
    decode_file_with_triggers as _decode_file_with_triggers,
    TriggerEvents,
)
from .events import Events

# Read from the installed distribution metadata rather than a literal. A
# hardcoded value silently goes stale: 0.4.0 shipped reporting "0.3.0".
try:
    __version__ = _package_version("evt3")
except PackageNotFoundError:  # pragma: no cover - source tree without install
    __version__ = "unknown"


def _wrap_events(events):
    return Events._from_trusted_arrays(
        x=events.x,
        y=events.y,
        p=events.p,
        t=events.t,
        geometry=events.sensor_size,
    )


def decode_file(path, *, threads=0):
    """Decode a file. threads=0: Auto; threads=1: serial; n>1: total thread cap."""

    return _wrap_events(_decode_file(path, threads=threads))


def decode_file_with_triggers(path, *, threads=0):
    """Decode an EVT 3.0 raw file and return events plus trigger events."""

    events, triggers = _decode_file_with_triggers(path, threads=threads)
    return _wrap_events(events), triggers


def decode_bytes(data, sensor_width=1280, sensor_height=720, *, threads=0):
    """Decode raw EVT 3.0 bytes and return NumPy-native events."""

    return _wrap_events(
        _decode_bytes(data, sensor_width=sensor_width, sensor_height=sensor_height,
                      threads=threads)
    )


def decode_file_batches(path, batch_bytes=8 * 1024 * 1024, *, threads=0):
    """Iterate over a raw or HDF5 file with bounded peak memory.

    Existing code can keep using :func:`decode_file`. Use this function only
    when the complete decoded recording does not need to remain in memory.
    """

    return FileDecoder(path, batch_bytes=batch_bytes, threads=threads)


def decode_file_batches_with_triggers(path, batch_bytes=8 * 1024 * 1024, *, threads=0):
    """Iterate over bounded batches of CD events and external triggers."""

    return FileDecoderWithTriggers(path, batch_bytes=batch_bytes, threads=threads)


class Decoder:
    """Stateful byte-stream decoder returning public :class:`Events` objects.

    threads=0 selects Auto; 1 runs serially; n>1 limits total decoding threads.
    Auto may use workers for even chunks of at least 64 KiB. For live previews,
    threads=1 provides a serial decoder; scheduling latency still depends on the OS.
    """

    def __init__(self, sensor_width=1280, sensor_height=720, *, threads=0):
        self._decoder = _Decoder(
            sensor_width=sensor_width, sensor_height=sensor_height, threads=threads
        )

    def feed(self, data):
        return _wrap_events(self._decoder.feed(data))

    def finish(self):
        return self._decoder.finish()

    def reset(self):
        return self._decoder.reset()

    @property
    def sensor_size(self):
        return self._decoder.sensor_size


class FileDecoder:
    """Bounded-memory iterator over decoded event batches."""

    def __init__(self, path, batch_bytes=8 * 1024 * 1024, *, threads=0):
        self._decoder = _decode_file_batches(path, batch_bytes=batch_bytes, threads=threads)

    def __iter__(self):
        return self

    def __next__(self):
        return _wrap_events(next(self._decoder))

    @property
    def sensor_width(self):
        return self._decoder.sensor_width

    @property
    def sensor_height(self):
        return self._decoder.sensor_height

    @property
    def sensor_size(self):
        return self._decoder.sensor_size


class FileDecoderWithTriggers:
    """Bounded-memory iterator yielding ``(Events, TriggerEvents)`` batches."""

    def __init__(self, path, batch_bytes=8 * 1024 * 1024, *, threads=0):
        self._decoder = _decode_file_batches_with_triggers(
            path, batch_bytes=batch_bytes, threads=threads
        )

    def __iter__(self):
        return self

    def __next__(self):
        events, triggers = next(self._decoder)
        return _wrap_events(events), triggers

    @property
    def sensor_width(self):
        return self._decoder.sensor_width

    @property
    def sensor_height(self):
        return self._decoder.sensor_height

    @property
    def sensor_size(self):
        return self._decoder.sensor_size


__all__ = [
    "PARALLEL_ENABLED",
    "decode_file",
    "decode_file_with_triggers",
    "decode_bytes",
    "decode_file_batches",
    "decode_file_batches_with_triggers",
    "Decoder",
    "FileDecoder",
    "FileDecoderWithTriggers",
    "Events",
    "TriggerEvents",
    "augur",
]
