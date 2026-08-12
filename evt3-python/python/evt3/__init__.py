"""
EVT3 - High-performance EVT 3.0 decoder for Prophesee event cameras.

This module provides a Rust-backed decoder for EVT 3.0 raw data files
with zero-copy numpy array support for efficient data handling.

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

from . import augur
from ._evt3 import (
    decode_bytes as _decode_bytes,
    decode_file as _decode_file,
    decode_file_with_triggers as _decode_file_with_triggers,
    TriggerEvents,
)
from .events import Events

__version__ = "0.3.0"


def _wrap_events(events):
    return Events._from_trusted_arrays(
        x=events.x,
        y=events.y,
        p=events.p,
        t=events.t,
        geometry=events.sensor_size,
    )


def decode_file(path):
    """Decode an EVT 3.0 raw file and return NumPy-native events."""

    return _wrap_events(_decode_file(path))


def decode_file_with_triggers(path):
    """Decode an EVT 3.0 raw file and return events plus trigger events."""

    events, triggers = _decode_file_with_triggers(path)
    return _wrap_events(events), triggers


def decode_bytes(data, sensor_width=1280, sensor_height=720):
    """Decode raw EVT 3.0 bytes and return NumPy-native events."""

    return _wrap_events(
        _decode_bytes(data, sensor_width=sensor_width, sensor_height=sensor_height)
    )


__all__ = [
    "decode_file",
    "decode_file_with_triggers",
    "decode_bytes",
    "Events",
    "TriggerEvents",
    "augur",
]
