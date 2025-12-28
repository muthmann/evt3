"""Tests for evt3 Python bindings."""

import pytest
import numpy as np


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
