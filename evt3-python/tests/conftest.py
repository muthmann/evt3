"""Pytest configuration and fixtures for evt3 tests."""

import pytest
from pathlib import Path


@pytest.fixture
def test_data_dir():
    """Path to test data directory."""
    # Navigate from evt3-python/tests/ to project root test_data/
    return Path(__file__).parent.parent.parent / "test_data"


@pytest.fixture
def sample_raw_file(test_data_dir):
    """Path to sample EVT3 raw file, if available."""
    path = test_data_dir / "laser.raw"
    if not path.exists():
        pytest.skip(f"Test file not found: {path}")
    return path


@pytest.fixture
def synthetic_evt3_bytes():
    """Generate minimal synthetic EVT3 data for testing."""
    import struct
    
    data = bytearray()
    
    # TIME_HIGH: type=0x8, time=0
    data.extend(struct.pack('<H', 0x8000))
    
    # TIME_LOW: type=0x6, time=100
    data.extend(struct.pack('<H', 0x6064))
    
    # ADDR_Y: type=0x0, y=200
    data.extend(struct.pack('<H', 0x00C8))
    
    # ADDR_X: type=0x2, pol=1, x=300
    data.extend(struct.pack('<H', 0x292C))  # 0b0010_1_00100101100
    
    # Another TIME_LOW: type=0x6, time=150
    data.extend(struct.pack('<H', 0x6096))
    
    # ADDR_X: type=0x2, pol=0, x=400
    data.extend(struct.pack('<H', 0x2190))  # 0b0010_0_00110010000
    
    # VECT_BASE_X: type=0x3, pol=1, x=500
    data.extend(struct.pack('<H', 0x39F4))  # 0b0011_1_00111110100
    
    # VECT_12: type=0x4, valid=0b000000111000 (events at x=503,504,505)
    data.extend(struct.pack('<H', 0x4038))
    
    return bytes(data)
