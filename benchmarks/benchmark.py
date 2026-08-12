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
import filecmp
import gc
import hashlib
import os
import statistics
import subprocess
import sys
import tempfile
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
        del events
        gc.collect()

    avg_time = sum(times) / len(times)
    return {
        "name": "Rust (Python)",
        "avg_time": avg_time,
        "min_time": min(times),
        "max_time": max(times),
        "event_count": event_count,
        "events_per_sec": event_count / avg_time,
    }


def benchmark_rust_python_batches(file_path: Path, iterations: int = 3) -> dict:
    """Benchmark bounded-memory Python decoding without retaining batches."""
    try:
        import evt3
    except ImportError:
        return None

    times = []
    event_count = 0
    for i in range(iterations):
        start = time.perf_counter()
        event_count = sum(
            len(events) for events in evt3.decode_file_batches(str(file_path))
        )
        elapsed = time.perf_counter() - start
        times.append(elapsed)
        print(f"  Rust (Python batches): Run {i+1}/{iterations}: {elapsed:.3f}s")

    avg_time = sum(times) / len(times)
    return {
        "name": "Rust (Python batches)",
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

    times = []
    event_count = 0

    for i in range(iterations):
        with tempfile.NamedTemporaryFile(suffix=".csv", delete=True) as tmp:
            elapsed = _run_checked(
                [str(cli_path), str(file_path), tmp.name, "--quiet"]
            )
            times.append(elapsed)

            # Count lines (events + 1 header)
            with open(tmp.name) as output:
                event_count = sum(1 for _ in output) - 1
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

    times = []
    event_count = 0

    for i in range(iterations):
        with tempfile.NamedTemporaryFile(suffix=".csv", delete=True) as tmp:
            elapsed = _run_checked([str(cpp_path), str(file_path), tmp.name])
            times.append(elapsed)

            # Count lines
            with open(tmp.name) as output:
                event_count = sum(1 for _ in output) - 1
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


def _run_checked(command: list) -> float:
    """Run one timed command and fail when the decoder does not succeed."""
    start = time.perf_counter()
    result = subprocess.run(command, capture_output=True, text=True)
    elapsed = time.perf_counter() - start
    if result.returncode != 0:
        message = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"Command failed ({result.returncode}): {message}")
    return elapsed


def verify_csv_equivalence(
    file_path: Path, rust_path: Path, cpp_path: Path, prefix_mib: int
) -> str:
    """Compare Rust and C++ CSV bytes on a bounded input prefix."""
    with tempfile.TemporaryDirectory(prefix="evt3-csv-compare-") as directory:
        directory = Path(directory)
        prefix_path = directory / "prefix.raw"
        rust_output = directory / "rust.csv"
        cpp_output = directory / "cpp.csv"

        with file_path.open("rb") as source, prefix_path.open("wb") as target:
            target.write(source.read(prefix_mib * 1024 * 1024))

        _run_checked([str(rust_path), str(prefix_path), str(rust_output), "--quiet"])
        _run_checked([str(cpp_path), str(prefix_path), str(cpp_output)])
        if not filecmp.cmp(rust_output, cpp_output, shallow=False):
            raise RuntimeError("Rust and C++ CSV output differ on the input prefix")
        digest = hashlib.sha256()
        with rust_output.open("rb") as output:
            for chunk in iter(lambda: output.read(1024 * 1024), b""):
                digest.update(chunk)
        return digest.hexdigest()


def benchmark_cli_csv_comparison(
    file_path: Path,
    rust_path: Path,
    cpp_path: Path,
    iterations: int,
    verify_prefix_mib: int,
) -> None:
    """Compare Rust and C++ while both format full CSV output to os.devnull."""
    for name, path in (("Rust CLI", rust_path), ("C++ reference", cpp_path)):
        if not path.exists():
            raise FileNotFoundError(f"{name} not found at {path}")

    digest = verify_csv_equivalence(
        file_path, rust_path, cpp_path, verify_prefix_mib
    )
    commands = {
        "Rust CLI": [str(rust_path), str(file_path), os.devnull, "--quiet"],
        "C++ Reference": [str(cpp_path), str(file_path), os.devnull],
    }

    print("Warming both CLI implementations...")
    for command in commands.values():
        _run_checked(command)

    samples = {name: [] for name in commands}
    for iteration in range(iterations):
        order = list(commands)
        if iteration % 2:
            order.reverse()
        for name in order:
            elapsed = _run_checked(commands[name])
            samples[name].append(elapsed)
            print(f"  {name}: Run {iteration + 1}/{iterations}: {elapsed:.3f}s")

    rust_mean = statistics.mean(samples["Rust CLI"])
    cpp_mean = statistics.mean(samples["C++ Reference"])
    print("\nLIKE-FOR-LIKE CSV COMPARISON")
    print(f"Verified prefix: {verify_prefix_mib} MiB, SHA-256 {digest}")
    print(f"{'Decoder':<20} {'Mean':>10} {'Median':>10} {'Range':>20}")
    for name in ("Rust CLI", "C++ Reference"):
        values = samples[name]
        print(
            f"{name:<20} {statistics.mean(values):>9.3f}s "
            f"{statistics.median(values):>9.3f}s "
            f"{min(values):>8.3f}-{max(values):.3f}s"
        )
    print(f"Rust speedup over C++: {cpp_mean / rust_mean:.2f}x")


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

    # Use C++ as the baseline when available, otherwise use the slowest result.
    cpp_result = next((r for r in results if r["name"] == "C++ Reference"), None)
    baseline_time = (
        cpp_result["avg_time"]
        if cpp_result is not None
        else max(r["avg_time"] for r in results if r)
    )

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
    parser.add_argument(
        "--csv-comparison-only",
        action="store_true",
        help="Run an alternating Rust/C++ full-CSV comparison to os.devnull",
    )
    parser.add_argument(
        "--cpp-binary",
        type=Path,
        default=Path(__file__).parent.parent / "cpp_reference" / "evt3_decoder",
        help="Path to the optimized C++ reference binary",
    )
    parser.add_argument(
        "--verify-prefix-mib",
        type=int,
        default=8,
        help="Input prefix size used for byte-for-byte CSV verification",
    )
    args = parser.parse_args()

    if args.iterations < 1:
        parser.error("--iterations must be at least 1")
    if args.verify_prefix_mib < 1:
        parser.error("--verify-prefix-mib must be at least 1")

    if not args.file.exists():
        print(f"Error: File not found: {args.file}")
        sys.exit(1)

    if args.csv_comparison_only:
        benchmark_cli_csv_comparison(
            args.file,
            Path(__file__).parent.parent / "target" / "release" / "evt3",
            args.cpp_binary,
            args.iterations,
            args.verify_prefix_mib,
        )
        return

    print(f"\nBenchmarking with: {args.file}")
    print(f"Iterations: {args.iterations}")
    print()

    results = []

    print("Running benchmarks...")
    results.append(benchmark_rust_python(args.file, args.iterations))
    results.append(benchmark_rust_python_batches(args.file, args.iterations))
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
