"""Tests for evt3 Python bindings."""

import struct

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

    def test_parallel_feed_keeps_returned_arrays_stable(self, synthetic_evt3_bytes):
        """Parallel packets and odd scalar packets produce identical arrays."""
        import evt3

        data = synthetic_evt3_bytes * 8193
        scalar = evt3.Decoder()
        expected = [scalar.feed(data[i:i + 4095]) for i in range(0, len(data), 4095)]
        scalar.finish()

        decoder = evt3.Decoder()
        first = decoder.feed(data[:65536])
        retained = {name: getattr(first, name).copy() for name in ("x", "y", "p", "t")}
        second = decoder.feed(data[65536:])
        decoder.finish()
        decoder.reset()
        decoder.feed(data)  # Previous arrays must outlive further decode work.
        for name in retained:
            assert np.array_equal(getattr(first, name), retained[name])
            actual = np.concatenate([getattr(first, name), getattr(second, name)])
            reference = np.concatenate([getattr(batch, name) for batch in expected])
            assert np.array_equal(actual, reference)

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


class TestParallelism:
    @pytest.mark.parametrize("threads", [0, 1, 2, 4])
    def test_every_entry_point_respects_threads_and_preserves_outputs(
        self, synthetic_evt3_bytes, tmp_path, threads
    ):
        import evt3

        if threads > 1 and not evt3.PARALLEL_ENABLED:
            pytest.skip("parallel feature disabled")
        payload = (synthetic_evt3_bytes + struct.pack("<H", 0xA301)) * 8192
        path = tmp_path / "thread-policy.raw"
        path.write_bytes(payload)
        expected, expected_triggers = evt3.decode_file_with_triggers(str(path), threads=1)

        def check(events):
            for field in ("x", "y", "p", "t"):
                np.testing.assert_array_equal(getattr(events, field), getattr(expected, field))

        check(evt3.decode_bytes(payload, threads=threads))
        check(evt3.decode_file(str(path), threads=threads))
        actual, triggers = evt3.decode_file_with_triggers(str(path), threads=threads)
        check(actual)
        for field in ("value", "id", "timestamp"):
            np.testing.assert_array_equal(getattr(triggers, field), getattr(expected_triggers, field))

        decoder = evt3.Decoder(threads=threads)
        # Include both an eligible even chunk and odd carried-byte boundaries.
        packets = [payload[:65536]] + [payload[i:i + 4095] for i in range(65536, len(payload), 4095)]
        retained = [decoder.feed(packet) for packet in packets]
        decoder.finish()
        for field in ("x", "y", "p", "t"):
            np.testing.assert_array_equal(
                np.concatenate([getattr(events, field) for events in retained]),
                getattr(expected, field),
            )
        decoder.reset()
        check(decoder.feed(payload))
        decoder.finish()

        for reader in (
            evt3.decode_file_batches(str(path), 65536, threads=threads),
            evt3.FileDecoder(str(path), 65536, threads=threads),
        ):
            batches = list(reader)
            for field in ("x", "y", "p", "t"):
                np.testing.assert_array_equal(
                    np.concatenate([getattr(events, field) for events in batches]),
                    getattr(expected, field),
                )
        for reader in (
            evt3.decode_file_batches_with_triggers(str(path), 65536, threads=threads),
            evt3.FileDecoderWithTriggers(str(path), 65536, threads=threads),
        ):
            batches = list(reader)
            np.testing.assert_array_equal(
                np.concatenate([events.t for events, _ in batches]), expected.t
            )
            np.testing.assert_array_equal(
                np.concatenate([triggers.timestamp for _, triggers in batches]),
                expected_triggers.timestamp,
            )

    def test_negative_threads_are_rejected(self):
        import evt3

        with pytest.raises((ValueError, OverflowError)):
            evt3.Decoder(threads=-1)

    def test_disabled_feature_rejects_explicit_worker_request(self):
        import evt3

        if evt3.PARALLEL_ENABLED:
            pytest.skip("requires a build without the parallel feature")
        with pytest.raises(ValueError, match="parallel"):
            evt3.Decoder(threads=2)


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
