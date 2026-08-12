"""NumPy-native event containers for decoded EVT3 data."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

import numpy as np


_U16_MAX = np.iinfo(np.uint16).max
_U64_MAX = np.iinfo(np.uint64).max


class Events:
    """Decoded event-camera events stored as stable NumPy arrays."""

    __slots__ = ("_x", "_y", "_polarity", "_timestamp", "_sensor_width", "_sensor_height")

    def __init__(
        self,
        x: np.ndarray,
        y: np.ndarray,
        polarity: np.ndarray,
        timestamp: np.ndarray,
        sensor_width: int,
        sensor_height: int,
    ) -> None:
        self._x = x
        self._y = y
        self._polarity = polarity
        self._timestamp = timestamp
        self._sensor_width = sensor_width
        self._sensor_height = sensor_height

    @classmethod
    def from_arrays(
        cls,
        *,
        x: Any,
        y: Any,
        p: Any | None = None,
        polarity: Any | None = None,
        t: Any | None = None,
        timestamp: Any | None = None,
        geometry: tuple[int, int],
        copy: bool = False,
        time_unit: str = "us",
    ) -> "Events":
        """Build an event container from existing NumPy-compatible arrays."""

        if time_unit != "us":
            raise ValueError("time_unit must be 'us'")
        if p is not None and polarity is not None:
            raise ValueError("provide only one of p or polarity")
        if t is not None and timestamp is not None:
            raise ValueError("provide only one of t or timestamp")
        if p is None:
            p = polarity
        if t is None:
            t = timestamp
        if p is None:
            raise ValueError("p or polarity is required")
        if t is None:
            raise ValueError("t or timestamp is required")

        width, height = _validate_geometry(geometry)
        arrays = _as_event_arrays(x, y, p, t)
        _validate_shapes(arrays)

        if copy:
            x_arr = _normalize_uint16("x", arrays["x"])
            y_arr = _normalize_uint16("y", arrays["y"])
            p_arr = _normalize_polarity(arrays["p"])
            t_arr = _normalize_timestamp(arrays["t"])
        else:
            x_arr = _require_dtype("x", arrays["x"], np.uint16)
            y_arr = _require_dtype("y", arrays["y"], np.uint16)
            p_arr = _require_one_dtype("p", arrays["p"], (np.uint8, np.bool_))
            t_arr = _require_one_dtype("t", arrays["t"], (np.uint64, np.int64))
            if p_arr.dtype == np.bool_:
                p_arr = p_arr.astype(np.uint8, copy=False)

        _validate_geometry_bounds(x_arr, y_arr, width, height)
        _validate_timestamp_values(t_arr)
        _validate_timestamps_nondecreasing(t_arr)
        _validate_polarity_values(p_arr)

        if copy:
            x_arr = np.asarray(x_arr, dtype=np.uint16, order="C")
            y_arr = np.asarray(y_arr, dtype=np.uint16, order="C")
            p_arr = np.asarray(p_arr, dtype=np.uint8, order="C")
            t_arr = np.asarray(t_arr, dtype=np.uint64, order="C")

        return cls(x_arr, y_arr, p_arr, t_arr, width, height)

    @classmethod
    def _from_trusted_arrays(
        cls,
        *,
        x: np.ndarray,
        y: np.ndarray,
        p: np.ndarray,
        t: np.ndarray,
        geometry: tuple[int, int],
    ) -> "Events":
        """Wrap arrays produced by the Rust decoder without redundant scans."""

        width, height = geometry
        return cls(x, y, p, t, int(width), int(height))

    @property
    def x(self) -> np.ndarray:
        return self._x

    @property
    def y(self) -> np.ndarray:
        return self._y

    @property
    def polarity(self) -> np.ndarray:
        return self._polarity

    @property
    def p(self) -> np.ndarray:
        return self._polarity

    @property
    def timestamp(self) -> np.ndarray:
        return self._timestamp

    @property
    def t(self) -> np.ndarray:
        return self._timestamp

    @property
    def sensor_width(self) -> int:
        return self._sensor_width

    @property
    def sensor_height(self) -> int:
        return self._sensor_height

    @property
    def sensor_size(self) -> tuple[int, int]:
        return (self._sensor_width, self._sensor_height)

    def to_dict(self) -> dict[str, np.ndarray]:
        return {
            "x": self._x,
            "y": self._y,
            "polarity": self._polarity,
            "timestamp": self._timestamp,
        }

    def __len__(self) -> int:
        return int(self._x.shape[0])

    def __repr__(self) -> str:
        return f"Events(count={len(self)}, sensor={self.sensor_width}x{self.sensor_height})"


def _as_event_arrays(x: Any, y: Any, p: Any, t: Any) -> dict[str, np.ndarray]:
    return {
        "x": np.asarray(x),
        "y": np.asarray(y),
        "p": np.asarray(p),
        "t": np.asarray(t),
    }


def _validate_geometry(geometry: tuple[int, int] | None) -> tuple[int, int]:
    if geometry is None:
        raise ValueError("geometry is required")
    if len(geometry) != 2:
        raise ValueError("geometry must be a (width, height) pair")
    width, height = geometry
    if not isinstance(width, (int, np.integer)) or not isinstance(height, (int, np.integer)):
        raise TypeError("geometry width and height must be integers")
    width = int(width)
    height = int(height)
    if width <= 0 or height <= 0:
        raise ValueError("geometry width and height must be positive")
    if width > _U16_MAX or height > _U16_MAX:
        raise ValueError("geometry width and height must fit uint16")
    return width, height


def _validate_shapes(arrays: Mapping[str, np.ndarray]) -> None:
    lengths = {}
    for name, array in arrays.items():
        if array.ndim != 1:
            raise ValueError(f"{name} must be a one-dimensional array")
        lengths[name] = array.shape[0]

    expected = lengths["x"]
    for name, length in lengths.items():
        if length != expected:
            raise ValueError(
                f"{name} length {length} does not match x length {expected}"
            )


def _require_dtype(name: str, array: np.ndarray, dtype: type[np.generic]) -> np.ndarray:
    if array.dtype != np.dtype(dtype):
        raise TypeError(f"{name} must have dtype {np.dtype(dtype).name} when copy=False")
    return array


def _require_one_dtype(
    name: str,
    array: np.ndarray,
    dtypes: tuple[type[np.generic], ...],
) -> np.ndarray:
    if array.dtype not in {np.dtype(dtype) for dtype in dtypes}:
        allowed = " or ".join(np.dtype(dtype).name for dtype in dtypes)
        raise TypeError(f"{name} must have dtype {allowed} when copy=False")
    return array


def _require_integer_array(name: str, array: np.ndarray) -> None:
    if not np.issubdtype(array.dtype, np.integer) and array.dtype != np.bool_:
        raise TypeError(f"{name} must be bool or integer, got {array.dtype}")


def _normalize_uint16(name: str, array: np.ndarray) -> np.ndarray:
    _require_integer_array(name, array)
    if array.size:
        min_value = int(array.min())
        max_value = int(array.max())
        if min_value < 0 or max_value > _U16_MAX:
            raise ValueError(f"{name} contains values outside uint16 range")
    return np.asarray(array, dtype=np.uint16, order="C")


def _normalize_polarity(array: np.ndarray) -> np.ndarray:
    _require_integer_array("p", array)
    return (np.asarray(array) != 0).astype(np.uint8, copy=False)


def _normalize_timestamp(array: np.ndarray) -> np.ndarray:
    _require_integer_array("t", array)
    if array.size and np.issubdtype(array.dtype, np.signedinteger):
        min_value = int(array.min())
        if min_value < 0:
            raise ValueError("t contains negative timestamps")
    return np.asarray(array, dtype=np.uint64, order="C")


def _validate_geometry_bounds(
    x: np.ndarray,
    y: np.ndarray,
    width: int,
    height: int,
) -> None:
    if x.size and bool(np.any(x >= width)):
        first = int(np.nonzero(x >= width)[0][0])
        raise ValueError(f"x contains value {int(x[first])} outside geometry width {width}")
    if y.size and bool(np.any(y >= height)):
        first = int(np.nonzero(y >= height)[0][0])
        raise ValueError(f"y contains value {int(y[first])} outside geometry height {height}")


def _validate_timestamp_values(t: np.ndarray) -> None:
    _require_integer_array("t", t)
    if t.size and np.issubdtype(t.dtype, np.signedinteger) and int(t.min()) < 0:
        raise ValueError("t contains negative timestamps")
    if t.size and np.issubdtype(t.dtype, np.unsignedinteger) and int(t.max()) > _U64_MAX:
        raise ValueError("t contains values outside uint64 range")


def _validate_timestamps_nondecreasing(t: np.ndarray) -> None:
    if t.size < 2:
        return
    decreases = np.nonzero(t[1:] < t[:-1])[0]
    if decreases.size:
        first = int(decreases[0] + 1)
        raise ValueError(f"t must be nondecreasing; first decrease at index {first}")


def _validate_polarity_values(p: np.ndarray) -> None:
    _require_integer_array("p", p)
    if p.dtype == np.bool_ or p.size == 0:
        return
    invalid = np.nonzero((p != 0) & (p != 1))[0]
    if invalid.size:
        first = int(invalid[0])
        raise ValueError(f"p contains value {int(p[first])}; expected 0/1 polarity values")
