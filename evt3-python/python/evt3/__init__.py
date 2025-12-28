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

from ._evt3 import (
    decode_file,
    decode_file_with_triggers,
    decode_bytes,
    Events,
    TriggerEvents,
)

__version__ = "0.1.0"
__all__ = [
    "decode_file",
    "decode_file_with_triggers", 
    "decode_bytes",
    "Events",
    "TriggerEvents",
]
