# Contributing to EVT3 Decoder

Thank you for your interest in contributing! This document provides guidelines for contributing to the EVT3 decoder project.

## Development Setup

### Prerequisites

- **Rust**: 1.70+ ([rustup.rs](https://rustup.rs/))
- **Python**: 3.8+ with pip
- **maturin**: For building Python bindings

### Getting Started

```bash
# Clone the repository
git clone https://github.com/your-username/evt3.git
cd evt3

# Build and test Rust code
cargo build
cargo test

# Set up Python development environment
cd evt3-python
python -m venv .venv
source .venv/bin/activate  # or `.venv\Scripts\activate` on Windows
pip install maturin pytest numpy pandas

# Build Python bindings in development mode
maturin develop

# Run Python tests
pytest
```

## Project Structure

```
evt3/
├── evt3-core/      # Core Rust decoder library
├── evt3-cli/       # Command-line tool
├── evt3-python/    # Python bindings (PyO3)
├── benchmarks/     # Benchmark suite
└── tests/          # Integration tests
```

## Making Changes

### Code Style

**Rust:**
- Follow standard Rust formatting: `cargo fmt`
- Ensure no clippy warnings: `cargo clippy`
- Write doc comments for public APIs

**Python:**
- Follow PEP 8 style guidelines
- Use type hints where appropriate
- Write docstrings for public functions

### Testing

Before submitting a PR, ensure all tests pass:

```bash
# Rust tests
cargo test --all

# Python tests (requires maturin develop first)
cd evt3-python && pytest

# Integration test against C++ reference
./tests/integration_test.sh
```

### Adding New Features

1. **Open an issue first** to discuss the feature
2. Write tests for new functionality
3. Update documentation as needed
4. Add entries to CHANGELOG.md

## Pull Request Process

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes with clear commit messages
4. Ensure tests pass and code is formatted
5. Push and open a Pull Request

### PR Checklist

- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] `cargo fmt` and `cargo clippy` pass
- [ ] CHANGELOG.md updated (for user-facing changes)

## Benchmarking

When making performance-related changes, run benchmarks:

```bash
# Rust benchmarks
cargo bench

# Python benchmarks
python benchmarks/benchmark.py
```

## Release Process

See [RELEASING.md](RELEASING.md) for release procedures (maintainers only).

## Getting Help

- Open an issue for bugs or feature requests
- Discussions for questions and ideas

## License

By contributing, you agree that your contributions will be licensed under the MIT OR Apache-2.0 license.
