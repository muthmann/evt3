"""Client helpers for publishing NumPy event arrays into Augur."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
import importlib.metadata
import json
import socket
from typing import Any, BinaryIO

import numpy as np


DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 57295
PACKED_EVENT_RECORD_BYTES = 14
PROTOCOL_VERSION = 1
DEFAULT_CHUNK_EVENTS = 262_144
_PACKED_DTYPE = np.dtype(
    [
        ("x", "<u2"),
        ("y", "<u2"),
        ("p", "u1"),
        ("pad", "u1"),
        ("t", "<u8"),
    ],
    align=False,
)


class AugurProtocolError(RuntimeError):
    """Raised when Augur rejects or violates the Python ingress protocol."""


@dataclass(frozen=True)
class _EventView:
    x: np.ndarray
    y: np.ndarray
    p: np.ndarray
    t: np.ndarray
    geometry: tuple[int, int]

    @property
    def event_count(self) -> int:
        return int(self.x.shape[0])


def connect(
    *,
    host: str = DEFAULT_HOST,
    port: int = DEFAULT_PORT,
    timeout: float | None = None,
) -> "AugurSession":
    """Open a reusable Augur ingress session."""

    return AugurSession(host=host, port=port, timeout=timeout)


def publish_events(
    events: Any = None,
    *,
    x: Any = None,
    y: Any = None,
    p: Any = None,
    t: Any = None,
    geometry: tuple[int, int] | None = None,
    name: str | None = None,
    host: str = DEFAULT_HOST,
    port: int = DEFAULT_PORT,
    time_unit: str = "us",
    chunk_events: int = DEFAULT_CHUNK_EVENTS,
    validate: bool = True,
    timeout: float | None = None,
) -> None:
    """Publish event arrays to a running Augur loopback ingress listener."""

    view = _normalize_event_input(
        events=events,
        x=x,
        y=y,
        p=p,
        t=t,
        geometry=geometry,
        time_unit=time_unit,
        validate=validate,
    )
    chunk_events = _validate_chunk_events(chunk_events)
    with connect(host=host, port=port, timeout=timeout) as session:
        session._publish_view(view, name=name, chunk_events=chunk_events)


class AugurSession:
    """Reusable TCP session for Augur Python event ingress."""

    def __init__(
        self,
        *,
        host: str = DEFAULT_HOST,
        port: int = DEFAULT_PORT,
        timeout: float | None = None,
    ) -> None:
        if host != DEFAULT_HOST:
            raise ValueError(
                "Augur Python ingress is loopback-only; host must be 127.0.0.1"
            )
        self.host = host
        self.port = int(port)
        self.timeout = timeout
        self._socket = socket.create_connection((self.host, self.port), timeout=timeout)
        self._file = self._socket.makefile("rwb", buffering=0)
        self._max_chunk_events = DEFAULT_CHUNK_EVENTS
        self._handshake()

    def __enter__(self) -> "AugurSession":
        return self

    def __exit__(self, exc_type: object, exc: object, tb: object) -> None:
        self.close()

    def close(self) -> None:
        file = getattr(self, "_file", None)
        sock = getattr(self, "_socket", None)
        self._file = None
        self._socket = None
        if file is not None:
            file.close()
        if sock is not None:
            sock.close()

    def publish_events(
        self,
        events: Any = None,
        *,
        x: Any = None,
        y: Any = None,
        p: Any = None,
        t: Any = None,
        geometry: tuple[int, int] | None = None,
        name: str | None = None,
        time_unit: str = "us",
        chunk_events: int = DEFAULT_CHUNK_EVENTS,
        validate: bool = True,
    ) -> None:
        """Publish one event dataset over this session."""

        view = _normalize_event_input(
            events=events,
            x=x,
            y=y,
            p=p,
            t=t,
            geometry=geometry,
            time_unit=time_unit,
            validate=validate,
        )
        chunk_events = _validate_chunk_events(chunk_events)
        self._publish_view(view, name=name, chunk_events=chunk_events)

    def _publish_view(
        self,
        view: _EventView,
        *,
        name: str | None,
        chunk_events: int,
    ) -> None:
        chunk_events = min(chunk_events, self._max_chunk_events)

        start = int(view.t[0]) if view.event_count else 0
        end = int(view.t[-1]) if view.event_count else start
        self._send_json(
            {
                "type": "start_events",
                "name": name,
                "geometry": [view.geometry[0], view.geometry[1]],
                "event_count": view.event_count,
                "time_unit": "us",
                "record_format": "packed_xypt_v1",
                "record_bytes": PACKED_EVENT_RECORD_BYTES,
                "timestamp_start_us": start,
                "timestamp_end_us": end,
            }
        )
        _expect_type(self._read_json(), "start_ok")

        for start_idx in range(0, view.event_count, chunk_events):
            end_idx = min(start_idx + chunk_events, view.event_count)
            packed = _pack_chunk(
                view.x[start_idx:end_idx],
                view.y[start_idx:end_idx],
                view.p[start_idx:end_idx],
                view.t[start_idx:end_idx],
            )
            payload = memoryview(packed).cast("B")
            events_in_batch = end_idx - start_idx
            self._send_json(
                {
                    "type": "event_batch",
                    "events": events_in_batch,
                    "bytes": packed.nbytes,
                }
            )
            self._write(payload)
            reply = self._read_json()
            _expect_type(reply, "batch_ok")
            if int(reply.get("events", -1)) != events_in_batch:
                raise AugurProtocolError(
                    f"Augur acknowledged {reply.get('events')} events for a "
                    f"{events_in_batch}-event batch"
                )

        self._send_json({"type": "finish_events"})
        _expect_type(self._read_json(), "finish_ok")

    def _handshake(self) -> None:
        self._send_json(
            {
                "type": "hello",
                "protocol": PROTOCOL_VERSION,
                "client": "evt3-python",
                "client_version": _client_version(),
            }
        )
        reply = self._read_json()
        if reply.get("type") == "error":
            raise AugurProtocolError(
                f"Augur connector refused protocol {PROTOCOL_VERSION}: "
                f"{reply.get('message', reply.get('code', 'unknown error'))}"
            )
        _expect_type(reply, "hello_ok")
        protocol = reply.get("protocol")
        if protocol != PROTOCOL_VERSION:
            raise AugurProtocolError(
                f"Augur replied with protocol {protocol}; expected {PROTOCOL_VERSION}"
            )
        max_chunk_events = int(reply.get("max_chunk_events", DEFAULT_CHUNK_EVENTS))
        self._max_chunk_events = _validate_chunk_events(max_chunk_events)

    def _send_json(self, message: Mapping[str, Any]) -> None:
        payload = json.dumps(message, separators=(",", ":")).encode("utf-8") + b"\n"
        self._write(payload)

    def _write(self, payload: bytes | memoryview) -> None:
        if self._file is None:
            raise AugurProtocolError("Augur session is closed")
        self._file.write(payload)

    def _read_json(self) -> dict[str, Any]:
        if self._file is None:
            raise AugurProtocolError("Augur session is closed")
        return _read_json_line(self._file)


def _normalize_event_input(
    *,
    events: Any,
    x: Any,
    y: Any,
    p: Any,
    t: Any,
    geometry: tuple[int, int] | None,
    time_unit: str,
    validate: bool,
) -> _EventView:
    if time_unit != "us":
        raise ValueError("time_unit must be 'us'")
    if events is not None and any(value is not None for value in (x, y, p, t)):
        raise ValueError("provide either events or explicit x/y/p/t arrays, not both")

    if events is not None:
        x, y, p, t = _arrays_from_events(events)
        if geometry is None:
            geometry = getattr(events, "sensor_size", None)
    elif any(value is None for value in (x, y, p, t)):
        raise ValueError("x, y, p, and t are required when events is not provided")

    width, height = _validate_geometry(geometry)
    x_arr = np.asarray(x)
    y_arr = np.asarray(y)
    p_arr = np.asarray(p)
    t_arr = np.asarray(t)
    _validate_shapes({"x": x_arr, "y": y_arr, "p": p_arr, "t": t_arr})
    _validate_integer_dtype("x", x_arr)
    _validate_integer_dtype("y", y_arr)
    _validate_integer_dtype("p", p_arr, allow_bool=True)
    _validate_integer_dtype("t", t_arr)

    if validate:
        _validate_range("x", x_arr, minimum=0, maximum=width - 1)
        _validate_range("y", y_arr, minimum=0, maximum=height - 1)
        _validate_range("t", t_arr, minimum=0, maximum=np.iinfo(np.uint64).max)
        _validate_timestamps_nondecreasing(t_arr)

    return _EventView(x=x_arr, y=y_arr, p=p_arr, t=t_arr, geometry=(width, height))


def _arrays_from_events(events: Any) -> tuple[Any, Any, Any, Any]:
    if isinstance(events, Mapping):
        try:
            return events["x"], events["y"], events["p"], events["t"]
        except KeyError as exc:
            raise ValueError("events mapping must contain x, y, p, and t keys") from exc
    try:
        return events.x, events.y, events.p, events.t
    except AttributeError as exc:
        raise ValueError("events must expose x, y, p, and t arrays") from exc


def _validate_geometry(geometry: tuple[int, int] | None) -> tuple[int, int]:
    if geometry is None:
        raise ValueError("geometry is required when publishing raw NumPy arrays")
    if len(geometry) != 2:
        raise ValueError("geometry must be a (width, height) pair")
    width, height = geometry
    if not isinstance(width, (int, np.integer)) or not isinstance(height, (int, np.integer)):
        raise TypeError("geometry width and height must be integers")
    width = int(width)
    height = int(height)
    if width <= 0 or height <= 0:
        raise ValueError("geometry width and height must be positive")
    if width > np.iinfo(np.uint16).max or height > np.iinfo(np.uint16).max:
        raise ValueError("geometry width and height must fit uint16")
    return width, height


def _validate_shapes(arrays: Mapping[str, np.ndarray]) -> None:
    lengths = {}
    for name, array in arrays.items():
        if array.ndim != 1:
            raise ValueError(f"{name} must be a one-dimensional array")
        lengths[name] = int(array.shape[0])
    expected = lengths["x"]
    for name, length in lengths.items():
        if length != expected:
            raise ValueError(f"{name} length {length} does not match x length {expected}")


def _validate_integer_dtype(name: str, array: np.ndarray, *, allow_bool: bool = False) -> None:
    if allow_bool and array.dtype == np.bool_:
        return
    if not np.issubdtype(array.dtype, np.integer):
        if name == "p":
            raise TypeError(f"p must be bool or integer polarity values, got {array.dtype}")
        raise TypeError(f"{name} must be an integer array, got {array.dtype}")


def _validate_range(name: str, array: np.ndarray, *, minimum: int, maximum: int) -> None:
    if array.size == 0:
        return
    if np.issubdtype(array.dtype, np.signedinteger) and int(array.min()) < minimum:
        first = int(np.nonzero(array < minimum)[0][0])
        raise ValueError(f"{name} contains value {int(array[first])} below {minimum}")
    if int(array.max()) > maximum:
        first = int(np.nonzero(array > maximum)[0][0])
        if name == "x":
            raise ValueError(
                f"x contains value {int(array[first])} "
                f"outside geometry width {maximum + 1}"
            )
        if name == "y":
            raise ValueError(
                f"y contains value {int(array[first])} "
                f"outside geometry height {maximum + 1}"
            )
        raise ValueError(f"{name} contains value {int(array[first])} above {maximum}")


def _validate_timestamps_nondecreasing(t: np.ndarray) -> None:
    if t.size < 2:
        return
    decreases = np.nonzero(t[1:] < t[:-1])[0]
    if decreases.size:
        first = int(decreases[0] + 1)
        raise ValueError(f"t must be nondecreasing; first decrease at index {first}")


def _validate_chunk_events(chunk_events: int) -> int:
    if not isinstance(chunk_events, (int, np.integer)):
        raise TypeError("chunk_events must be an integer")
    chunk_events = int(chunk_events)
    if chunk_events <= 0:
        raise ValueError("chunk_events must be positive")
    return chunk_events


def _pack_chunk(x: np.ndarray, y: np.ndarray, p: np.ndarray, t: np.ndarray) -> np.ndarray:
    packed = np.zeros(x.shape[0], dtype=_PACKED_DTYPE)
    packed["x"] = np.asarray(x, dtype="<u2")
    packed["y"] = np.asarray(y, dtype="<u2")
    packed["p"] = np.asarray(p != 0, dtype=np.uint8)
    packed["t"] = np.asarray(t, dtype="<u8")
    return packed


def _read_json_line(file: BinaryIO) -> dict[str, Any]:
    line = file.readline()
    if not line:
        raise AugurProtocolError("Augur closed the connection")
    try:
        message = json.loads(line.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise AugurProtocolError(f"Augur sent invalid JSON: {exc}") from exc
    if not isinstance(message, dict):
        raise AugurProtocolError("Augur sent a non-object JSON message")
    return message


def _expect_type(message: Mapping[str, Any], expected_type: str) -> None:
    if message.get("type") == "error":
        raise AugurProtocolError(
            f"Augur returned error {message.get('code', 'unknown')}: "
            f"{message.get('message', '')}"
        )
    if message.get("type") != expected_type:
        raise AugurProtocolError(
            f"Augur sent {message.get('type')!r}; expected {expected_type!r}"
        )


def _client_version() -> str:
    try:
        return importlib.metadata.version("evt3")
    except importlib.metadata.PackageNotFoundError:
        return "0.3.0"
