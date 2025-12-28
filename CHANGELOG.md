# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2024-12-28

### Added

- Initial release
- Full EVT 3.0 format support including vectorized events (VECT_12, VECT_8)
- CLI tool (`evt3-decode`) with CSV and binary output formats
- Python bindings with zero-copy NumPy array access
- Customizable field order for CSV output
- External trigger event support
- File header parsing for sensor metadata
- TIME_HIGH loop detection for recordings >16.7 seconds
- Comprehensive test suite validated against C++ reference implementation

### Performance

- 1.6x faster than C++ reference implementation
- 9.2M events/second throughput on Apple M1
