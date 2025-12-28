#!/usr/bin/env python3
"""
EVT3 Decoder Benchmark Suite

Compares performance of:
- Rust decoder (via evt3 Python package)
- C++ reference decoder (if compiled)

Usage:
    python benchmark.py [--file PATH] [--iterations N]
"""

import argparse
import subprocess
import sys
import time
from pathlib import Path


def get_file_size_mb(path: Path) -> float:
    """Get file size in megabytes."""
    return path.stat().st_size / (1024 * 1024)


def benchmark_rust_python(file_path: Path, iterations: int = 3) -> dict:
    """Benchmark Rust decoder via Python bindings."""
    try:
        import evt3
    except ImportError:
        print("evt3 package not installed. Run: cd evt3-python && maturin develop")
        return None

    times = []
    event_count = 0

    for i in range(iterations):
        start = time.perf_counter()
        events = evt3.decode_file(str(file_path))
        elapsed = time.perf_counter() - start
        times.append(elapsed)
        event_count = len(events)
        print(f"  Rust (Python): Run {i+1}/{iterations}: {elapsed:.3f}s")

    avg_time = sum(times) / len(times)
    return {
        "name": "Rust (Python)",
        "avg_time": avg_time,
        "min_time": min(times),
        "max_time": max(times),
        "event_count": event_count,
        "events_per_sec": event_count / avg_time,
    }


def benchmark_rust_cli(file_path: Path, iterations: int = 3) -> dict:
    """Benchmark Rust CLI decoder."""
    cli_path = Path(__file__).parent.parent / "target" / "release" / "evt3"
    if not cli_path.exists():
        print(f"  CLI not found at {cli_path}. Run: cargo build --release")
        return None

    import tempfile
    times = []
    event_count = 0

    for i in range(iterations):
        with tempfile.NamedTemporaryFile(suffix=".csv", delete=True) as tmp:
            start = time.perf_counter()
            result = subprocess.run(
                [str(cli_path), str(file_path), tmp.name, "--quiet"],
                capture_output=True,
                text=True,
            )
            elapsed = time.perf_counter() - start
            times.append(elapsed)
            
            # Count lines (events + 1 header)
            event_count = sum(1 for _ in open(tmp.name)) - 1
            print(f"  Rust CLI: Run {i+1}/{iterations}: {elapsed:.3f}s")

    avg_time = sum(times) / len(times)
    return {
        "name": "Rust CLI",
        "avg_time": avg_time,
        "min_time": min(times),
        "max_time": max(times),
        "event_count": event_count,
        "events_per_sec": event_count / avg_time,
    }


def benchmark_cpp_reference(file_path: Path, iterations: int = 3) -> dict:
    """Benchmark C++ reference decoder."""
    cpp_path = Path(__file__).parent.parent / "cpp_reference" / "evt3_decoder"
    if not cpp_path.exists():
        print(f"  C++ decoder not found at {cpp_path}")
        return None

    import tempfile
    times = []
    event_count = 0

    for i in range(iterations):
        with tempfile.NamedTemporaryFile(suffix=".csv", delete=True) as tmp:
            start = time.perf_counter()
            result = subprocess.run(
                [str(cpp_path), str(file_path), tmp.name],
                capture_output=True,
                text=True,
            )
            elapsed = time.perf_counter() - start
            times.append(elapsed)
            
            # Count lines
            event_count = sum(1 for _ in open(tmp.name)) - 1
            print(f"  C++ reference: Run {i+1}/{iterations}: {elapsed:.3f}s")

    avg_time = sum(times) / len(times)
    return {
        "name": "C++ Reference",
        "avg_time": avg_time,
        "min_time": min(times),
        "max_time": max(times),
        "event_count": event_count,
        "events_per_sec": event_count / avg_time,
    }


def format_number(n: float) -> str:
    """Format large numbers with K/M suffix."""
    if n >= 1_000_000:
        return f"{n/1_000_000:.2f}M"
    elif n >= 1_000:
        return f"{n/1_000:.2f}K"
    else:
        return f"{n:.2f}"


def print_results(results: list, file_path: Path):
    """Print benchmark results in a nice table."""
    print("\n" + "=" * 70)
    print("BENCHMARK RESULTS")
    print("=" * 70)
    print(f"File: {file_path.name}")
    print(f"Size: {get_file_size_mb(file_path):.2f} MB")
    print()

    # Table header
    print(f"{'Decoder':<20} {'Avg Time':<12} {'Events/sec':<15} {'Speedup':<10}")
    print("-" * 60)

    # Find baseline (C++ if available, otherwise slowest)
    baseline_time = max(r["avg_time"] for r in results if r)
    
    for result in results:
        if result is None:
            continue
        
        speedup = baseline_time / result["avg_time"]
        print(
            f"{result['name']:<20} "
            f"{result['avg_time']:.3f}s{'':<6} "
            f"{format_number(result['events_per_sec'])}/s{'':<6} "
            f"{speedup:.2f}x"
        )

    print()
    print(f"Total events decoded: {format_number(results[0]['event_count'])}")


def main():
    parser = argparse.ArgumentParser(description="EVT3 Decoder Benchmark")
    parser.add_argument(
        "--file",
        type=Path,
        default=Path(__file__).parent.parent / "test_data" / "laser.raw",
        help="Path to EVT3 raw file",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=3,
        help="Number of iterations per benchmark",
    )
    args = parser.parse_args()

    if not args.file.exists():
        print(f"Error: File not found: {args.file}")
        sys.exit(1)

    print(f"\nBenchmarking with: {args.file}")
    print(f"Iterations: {args.iterations}")
    print()

    results = []

    print("Running benchmarks...")
    results.append(benchmark_rust_python(args.file, args.iterations))
    results.append(benchmark_rust_cli(args.file, args.iterations))
    results.append(benchmark_cpp_reference(args.file, args.iterations))

    # Filter out None results
    results = [r for r in results if r is not None]

    if results:
        print_results(results, args.file)
    else:
        print("No benchmarks were run successfully.")
        sys.exit(1)


if __name__ == "__main__":
    main()
