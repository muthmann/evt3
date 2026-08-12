"""Tests for evt3 Python bindings."""

import json
import socket
import struct
import threading

import pytest
import numpy as np


class TestPackageMetadata:
    """Tests for package-level metadata."""

    def test_version_matches_distribution_metadata(self):
        """`__version__` must not drift from the installed distribution.

        It was previously a hardcoded literal, and 0.4.0 shipped reporting
        "0.3.0" because the release bump missed it.
        """
        from importlib.metadata import version

        import evt3

        assert evt3.__version__ == version("evt3")


class TestDecodeBytes:
    """Tests for decode_bytes function."""

    def test_decode_synthetic_data(self, synthetic_evt3_bytes):
        """Test decoding synthetic EVT3 bytes."""
        import evt3
        
        events = evt3.decode_bytes(synthetic_evt3_bytes, sensor_width=1280, sensor_height=720)
        
        # Should have decoded some events
        assert len(events) > 0
        
        # Check first event (from ADDR_X)
        assert events.x[0] == 300
        assert events.y[0] == 200
        assert events.polarity[0] == 1
        assert events.timestamp[0] == 100

    def test_events_properties(self, synthetic_evt3_bytes):
        """Test Events object properties."""
        import evt3
        
        events = evt3.decode_bytes(synthetic_evt3_bytes)
        
        # Check sensor properties
        assert events.sensor_width == 1280
        assert events.sensor_height == 720
        assert events.sensor_size == (1280, 720)

    def test_numpy_array_types(self, synthetic_evt3_bytes):
        """Test that returned arrays have correct numpy dtypes."""
        import evt3
        
        events = evt3.decode_bytes(synthetic_evt3_bytes)
        
        assert events.x.dtype == np.uint16
        assert events.y.dtype == np.uint16
        assert events.polarity.dtype == np.uint8
        assert events.p.dtype == np.uint8  # Alias
        assert events.timestamp.dtype == np.uint64
        assert events.t.dtype == np.uint64  # Alias

    def test_numpy_arrays_are_stable(self, synthetic_evt3_bytes):
        """Repeated Events property access returns stable array objects."""
        import evt3
        from evt3 import _evt3

        events = evt3.decode_bytes(synthetic_evt3_bytes)
        assert events.x is events.x
        assert events.y is events.y
        assert events.p is events.p
        assert events.t is events.t
        assert events.polarity is events.p
        assert events.timestamp is events.t

        raw_events = _evt3.decode_bytes(synthetic_evt3_bytes)
        assert raw_events.x is raw_events.x
        assert raw_events.y is raw_events.y
        assert raw_events.p is raw_events.p
        assert raw_events.t is raw_events.t

    def test_to_dict(self, synthetic_evt3_bytes):
        """Test to_dict() returns proper dictionary."""
        import evt3
        
        events = evt3.decode_bytes(synthetic_evt3_bytes)
        d = events.to_dict()
        
        assert isinstance(d, dict)
        assert 'x' in d
        assert 'y' in d
        assert 'polarity' in d
        assert 'timestamp' in d
        
        # All arrays should have same length
        assert len(d['x']) == len(d['y']) == len(d['polarity']) == len(d['timestamp'])

    def test_repr(self, synthetic_evt3_bytes):
        """Test string representation."""
        import evt3
        
        events = evt3.decode_bytes(synthetic_evt3_bytes)
        repr_str = repr(events)
        
        assert 'Events' in repr_str
        assert '1280x720' in repr_str

    def test_stateful_decoder_matches_legacy_function(self, synthetic_evt3_bytes):
        """Chunk boundaries must not change the legacy decode result."""
        import evt3

        expected = evt3.decode_bytes(synthetic_evt3_bytes)
        decoder = evt3.Decoder()
        batches = [
            decoder.feed(synthetic_evt3_bytes[:5]),
            decoder.feed(synthetic_evt3_bytes[5:]),
        ]
        decoder.finish()

        assert isinstance(batches[0], evt3.Events)
        assert np.array_equal(np.concatenate([batch.x for batch in batches]), expected.x)
        assert np.array_equal(np.concatenate([batch.y for batch in batches]), expected.y)
        assert np.array_equal(np.concatenate([batch.p for batch in batches]), expected.p)
        assert np.array_equal(np.concatenate([batch.t for batch in batches]), expected.t)

    def test_file_batch_iterator_matches_decode_file(self, synthetic_evt3_bytes, tmp_path):
        """The optional bounded-memory workflow preserves the old API result."""
        import evt3

        path = tmp_path / "synthetic.raw"
        path.write_bytes(synthetic_evt3_bytes)
        expected = evt3.decode_file(str(path))
        decoder = evt3.decode_file_batches(str(path), batch_bytes=5)
        batches = list(decoder)

        assert decoder.sensor_size == expected.sensor_size
        assert all(isinstance(batch, evt3.Events) for batch in batches)
        assert np.array_equal(np.concatenate([batch.x for batch in batches]), expected.x)
        assert np.array_equal(np.concatenate([batch.y for batch in batches]), expected.y)
        assert np.array_equal(np.concatenate([batch.p for batch in batches]), expected.p)
        assert np.array_equal(np.concatenate([batch.t for batch in batches]), expected.t)

    def test_file_batch_iterator_with_triggers(self, synthetic_evt3_bytes, tmp_path):
        """The trigger-aware iterator exposes every decoded external trigger."""
        import evt3

        trigger_word = struct.pack("<H", 0xA301)  # channel 3, rising edge
        path = tmp_path / "synthetic-with-trigger.raw"
        path.write_bytes(synthetic_evt3_bytes + trigger_word)

        expected_events, expected_triggers = evt3.decode_file_with_triggers(str(path))
        decoder = evt3.decode_file_batches_with_triggers(str(path), batch_bytes=5)
        batches = list(decoder)
        event_batches = [events for events, _triggers in batches]
        trigger_batches = [triggers for _events, triggers in batches]

        assert decoder.sensor_size == expected_events.sensor_size
        assert all(isinstance(events, evt3.Events) for events in event_batches)
        assert all(isinstance(triggers, evt3.TriggerEvents) for triggers in trigger_batches)
        assert np.array_equal(
            np.concatenate([batch.x for batch in event_batches]), expected_events.x
        )
        assert np.array_equal(
            np.concatenate([batch.timestamp for batch in trigger_batches]),
            expected_triggers.timestamp,
        )
        assert np.array_equal(
            np.concatenate([batch.id for batch in trigger_batches]), expected_triggers.id
        )
        assert np.array_equal(
            np.concatenate([batch.value for batch in trigger_batches]),
            expected_triggers.value,
        )


class TestDecodeFile:
    """Tests for decode_file function (requires real test data)."""

    def test_decode_real_file(self, sample_raw_file):
        """Test decoding a real EVT3 file."""
        import evt3
        
        events = evt3.decode_file(str(sample_raw_file))
        
        # Check we got a lot of events (laser.raw has ~116M)
        assert len(events) > 100_000_000
        
        # Check metadata
        assert events.sensor_width == 1280
        assert events.sensor_height == 720

    def test_decode_with_triggers(self, sample_raw_file):
        """Test decode_file_with_triggers function."""
        import evt3
        
        events, triggers = evt3.decode_file_with_triggers(str(sample_raw_file))
        
        assert len(events) > 100_000_000

    def test_numpy_operations(self, sample_raw_file):
        """Test that numpy operations work on returned arrays."""
        import evt3
        
        events = evt3.decode_file(str(sample_raw_file))
        
        # Basic numpy operations
        x_mean = np.mean(events.x)
        y_mean = np.mean(events.y)
        
        assert 0 < x_mean < 1280
        assert 0 < y_mean < 720
        
        # Filtering
        on_events = events.polarity == 1
        assert np.sum(on_events) > 0
        
        # Timestamps should be monotonically increasing
        t_diff = np.diff(events.timestamp.astype(np.int64))
        assert np.all(t_diff >= 0), "Timestamps should be monotonic"


class TestErrorHandling:
    """Tests for error handling."""

    def test_file_not_found(self):
        """Test error when file doesn't exist."""
        import evt3
        
        with pytest.raises(IOError):
            evt3.decode_file("/nonexistent/path/to/file.raw")

    def test_empty_bytes(self):
        """Test decoding empty bytes."""
        import evt3
        
        events = evt3.decode_bytes(b"")
        assert len(events) == 0


class TestEventsFromArrays:
    """Tests for NumPy-native Events construction."""

    def test_from_arrays_copy_false_preserves_arrays(self):
        import evt3

        x = np.array([1, 2], dtype=np.uint16)
        y = np.array([3, 4], dtype=np.uint16)
        p = np.array([0, 1], dtype=np.uint8)
        t = np.array([10, 20], dtype=np.uint64)

        events = evt3.Events.from_arrays(
            x=x, y=y, p=p, t=t, geometry=(1280, 720), copy=False
        )

        assert events.x is x
        assert events.y is y
        assert events.p is p
        assert events.t is t
        assert events.sensor_size == (1280, 720)

    def test_from_arrays_copy_true_normalizes_integer_inputs(self):
        import evt3

        events = evt3.Events.from_arrays(
            x=np.array([1, 2], dtype=np.int32),
            y=np.array([3, 4], dtype=np.int64),
            p=np.array([0, 9], dtype=np.int16),
            t=np.array([10, 20], dtype=np.int64),
            geometry=(1280, 720),
            copy=True,
        )

        assert events.x.dtype == np.uint16
        assert events.y.dtype == np.uint16
        assert events.p.dtype == np.uint8
        assert events.t.dtype == np.uint64
        np.testing.assert_array_equal(events.p, np.array([0, 1], dtype=np.uint8))

    def test_from_arrays_rejects_unsorted_timestamps(self):
        import evt3

        with pytest.raises(ValueError, match="first decrease at index 2"):
            evt3.Events.from_arrays(
                x=np.array([1, 2, 3], dtype=np.uint16),
                y=np.array([1, 2, 3], dtype=np.uint16),
                p=np.array([0, 1, 0], dtype=np.uint8),
                t=np.array([10, 20, 15], dtype=np.uint64),
                geometry=(1280, 720),
            )

    def test_from_arrays_rejects_geometry_mismatch(self):
        import evt3

        with pytest.raises(ValueError, match="outside geometry width 2"):
            evt3.Events.from_arrays(
                x=np.array([2], dtype=np.uint16),
                y=np.array([0], dtype=np.uint16),
                p=np.array([1], dtype=np.uint8),
                t=np.array([0], dtype=np.uint64),
                geometry=(2, 2),
            )


class FakeAugurServer:
    """Tiny protocol-v1 test server."""

    def __init__(self, *, max_chunk_events=1_048_576):
        self.max_chunk_events = max_chunk_events
        self.messages = []
        self.payloads = []
        self._ready = threading.Event()
        self._thread = threading.Thread(target=self._serve, daemon=True)
        self._listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._listener.bind(("127.0.0.1", 0))
        self._listener.listen(1)
        self.port = self._listener.getsockname()[1]

    def __enter__(self):
        self._thread.start()
        self._ready.wait(timeout=2)
        return self

    def __exit__(self, exc_type, exc, tb):
        self._thread.join(timeout=2)
        self._listener.close()

    def _serve(self):
        self._ready.set()
        conn, _addr = self._listener.accept()
        with conn:
            file = conn.makefile("rwb", buffering=0)
            hello = self._read_json(file)
            self.messages.append(hello)
            self._write_json(
                file,
                {
                    "type": "hello_ok",
                    "protocol": 1,
                    "server": "augur",
                    "max_chunk_events": self.max_chunk_events,
                },
            )

            start = self._read_json(file)
            self.messages.append(start)
            self._write_json(file, {"type": "start_ok"})

            while True:
                message = self._read_json(file)
                self.messages.append(message)
                if message["type"] == "finish_events":
                    self._write_json(file, {"type": "finish_ok"})
                    return
                assert message["type"] == "event_batch"
                payload = file.read(message["bytes"])
                self.payloads.append(payload)
                self._write_json(file, {"type": "batch_ok", "events": message["events"]})

    @staticmethod
    def _read_json(file):
        return json.loads(file.readline().decode("utf-8"))

    @staticmethod
    def _write_json(file, message):
        file.write(json.dumps(message).encode("utf-8") + b"\n")


class TestAugurConnector:
    """Tests for the Python-to-Augur connector protocol."""

    def test_publish_events_sends_chunked_packed_records(self):
        import evt3

        events = evt3.Events.from_arrays(
            x=np.array([1, 2, 3, 4, 5], dtype=np.uint16),
            y=np.array([10, 20, 30, 40, 50], dtype=np.uint16),
            p=np.array([0, 1, 0, 1, 1], dtype=np.uint8),
            t=np.array([100, 200, 300, 400, 500], dtype=np.uint64),
            geometry=(1280, 720),
        )

        with FakeAugurServer(max_chunk_events=3) as server:
            evt3.augur.publish_events(
                events,
                port=server.port,
                name="unit-test",
                chunk_events=2,
            )

        assert [m["type"] for m in server.messages] == [
            "hello",
            "start_events",
            "event_batch",
            "event_batch",
            "event_batch",
            "finish_events",
        ]
        assert server.messages[1]["name"] == "unit-test"
        assert server.messages[1]["geometry"] == [1280, 720]
        assert server.messages[1]["event_count"] == 5
        assert [m["events"] for m in server.messages if m["type"] == "event_batch"] == [
            2,
            2,
            1,
        ]

        payload = b"".join(server.payloads)
        assert len(payload) == 5 * evt3.augur.PACKED_EVENT_RECORD_BYTES
        first = struct.unpack_from("<HHBBQ", payload, 0)
        last = struct.unpack_from("<HHBBQ", payload, 4 * evt3.augur.PACKED_EVENT_RECORD_BYTES)
        assert first == (1, 10, 0, 0, 100)
        assert last == (5, 50, 1, 0, 500)

    def test_publish_events_rejects_float_dtype(self):
        import evt3

        with pytest.raises(TypeError, match="got float32"):
            evt3.augur.publish_events(
                x=np.array([1], dtype=np.float32),
                y=np.array([1], dtype=np.uint16),
                p=np.array([1], dtype=np.uint8),
                t=np.array([1], dtype=np.uint64),
                geometry=(1280, 720),
                port=1,
            )


class TestPandasIntegration:
    """Tests for pandas integration."""

    def test_to_dataframe(self, synthetic_evt3_bytes):
        """Test creating DataFrame from events."""
        pytest.importorskip("pandas")
        import pandas as pd
        import evt3
        
        events = evt3.decode_bytes(synthetic_evt3_bytes)
        df = pd.DataFrame(events.to_dict())
        
        assert isinstance(df, pd.DataFrame)
        assert list(df.columns) == ['x', 'y', 'polarity', 'timestamp']
        assert len(df) == len(events)
