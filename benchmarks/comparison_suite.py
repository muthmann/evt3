"""Build and compare matching CSV and Python workloads against saved baselines."""

import gc
import fnmatch
import hashlib
import io
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CPP_URL = (
    "https://github.com/prophesee-ai/openeb/blob/main/standalone_samples/"
    "metavision_evt3_raw_file_decoder/metavision_evt3_raw_file_decoder.cpp"
)


def checked(command, **kwargs):
    result = subprocess.run(command, capture_output=True, text=True, **kwargs)
    if result.returncode:
        raise RuntimeError(
            f"Command failed ({result.returncode}): {command}\n"
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def copy_prefix(source, destination, limit):
    """Keep the text header and end a prefix on a complete payload word."""
    with Path(source).open("rb") as stream:
        header_end = 0
        while stream.read(1) == b"%":
            line = b"%" + stream.readline()
            header_end = stream.tell()
            if line.rstrip() == b"% end":
                break
        size = min(Path(source).stat().st_size, limit)
        if size < header_end + 2:
            raise ValueError("Input prefix must contain the header and at least one word")
        size -= (size - header_end) % 2
        stream.seek(0)
        with Path(destination).open("wb") as target:
            remaining = size
            while remaining:
                chunk = stream.read(min(remaining, 1 << 20))
                if not chunk:
                    raise RuntimeError("Input changed while its prefix was copied")
                target.write(chunk)
                remaining -= len(chunk)


def run_csv(binary, files, workers, rust, output=os.devnull, threads=None):
    def decode(path):
        command = [str(binary), str(path), str(output)]
        if rust:
            command.append("--quiet")
        if threads is not None:
            command.extend(["--threads", str(threads)])
        checked(command)

    start = time.perf_counter()
    if workers == 1:
        for path in files:
            decode(path)
    else:
        with ThreadPoolExecutor(max_workers=workers) as pool:
            list(pool.map(decode, files))
    return time.perf_counter() - start


def verify_csv(files, binaries, directory, prefix_bytes, threads=None):
    """Check each file prefix with all three executables before timing them."""
    verified = []
    for index, path in enumerate(files):
        prefix = directory / f"verify-{index}.raw"
        copy_prefix(path, prefix, prefix_bytes)
        digests = {}
        for name, binary in binaries.items():
            output = directory / "verify.csv"
            options = {"threads": threads} if name == "rust_current" and threads is not None else {}
            run_csv(binary, [prefix], 1, name != "prophesee_cpp", output, **options)
            digests[name] = sha256(output)
            output.unlink()
        if len(set(digests.values())) != 1:
            raise RuntimeError(f"CSV output differs for {path}: {digests}")
        verified.append({"file": str(path), "prefix_bytes": prefix.stat().st_size,
                         "csv_sha256": digests["prophesee_cpp"]})
    return verified


def sample_summary(samples):
    return {
        "samples_seconds": samples,
        "mean_seconds": statistics.mean(samples),
        "median_seconds": statistics.median(samples),
        "min_seconds": min(samples),
        "max_seconds": max(samples),
    }


def save_report(report, output):
    if output:
        output.parent.mkdir(parents=True, exist_ok=True)
        temporary = output.with_name(output.name + ".tmp")
        temporary.write_text(json.dumps(report, indent=2) + "\n")
        temporary.replace(output)


def compare_case(name, runners, iterations):
    """Warm every implementation, then rotate order to limit ordering bias."""
    samples = {name: [] for name in runners}
    signature = None
    events = None
    packet_samples = {label: [] for label in runners}
    print(f"\n{name}: warming {', '.join(runners)}", flush=True)

    def run_one(label):
        nonlocal signature, events
        result = runners[label]()
        if "signature" in result:
            if signature is None:
                signature = result["signature"]
            elif signature != result["signature"]:
                raise RuntimeError(f"Decoded events differ in {name}: {label}")
            events = result["event_count"]
        return result

    for label in runners:
        run_one(label)
    labels = list(runners)
    for iteration in range(iterations):
        offset = iteration % len(labels)
        for label in labels[offset:] + labels[:offset]:
            measured = run_one(label)
            elapsed = measured["seconds"]
            samples[label].append(elapsed)
            if measured.get("packet_latency_seconds", {}).get("count", 0):
                packet_samples[label].append(measured["packet_latency_seconds"])
            print(f"  {label}: {iteration + 1}/{iterations}: {elapsed:.6f}s", flush=True)
    results = {label: sample_summary(values) for label, values in samples.items()}
    current = results["rust_current"]["median_seconds"]
    speedups = {
        label: result["median_seconds"] / current
        for label, result in results.items() if label != "rust_current"
    }
    for label, packets in packet_samples.items():
        if packets:
            results[label]["packet_latency_samples"] = packets
    return {"name": name, "implementations": results, "current_speedup_vs": speedups,
            "current_time_reduction_vs": {label: 1 - 1 / ratio for label, ratio in speedups.items()},
            "event_count": events, "signature": signature}


def _event_signature(events):
    import numpy as np

    return {
        "count": len(events),
        "sums": [int(column.sum(dtype=np.uint64))
                 for column in (events.x, events.y, events.p, events.t)],
        "first_timestamp": int(events.t[0]) if len(events) else None,
        "last_timestamp": int(events.t[-1]) if len(events) else None,
    }


def _batch_signature(evt3, path, **options):
    total = {"count": 0, "sums": [0, 0, 0, 0],
             "first_timestamp": None, "last_timestamp": None}
    for events in evt3.decode_file_batches(str(path), **options):
        signature = _event_signature(events)
        if signature["count"]:
            if total["first_timestamp"] is None:
                total["first_timestamp"] = signature["first_timestamp"]
            total["last_timestamp"] = signature["last_timestamp"]
        total["count"] += signature["count"]
        total["sums"] = [(a + b) & ((1 << 64) - 1)
                         for a, b in zip(total["sums"], signature["sums"])]
    return total


def python_worker(request):
    # Each version is imported in a fresh process. Imports are outside the timer.
    sys.path.insert(0, request["package_dir"])
    import evt3

    options = {"threads": request["threads"]} if request.get("threads") is not None else {}
    if request.get("feed_chunk_bytes"):
        return _python_feed(evt3, request["files"][0], request["feed_chunk_bytes"], **options)
    files = request["files"]
    workers = request["workers"]
    batches = request["batches"]
    gc.collect()
    start = time.perf_counter()
    if batches:
        decode = lambda path: _batch_signature(evt3, path, **options)
    else:
        decode = lambda path: evt3.decode_file(str(path), **options)
    if workers == 1:
        outputs = [decode(path) for path in files]
    else:
        with ThreadPoolExecutor(max_workers=workers) as pool:
            outputs = list(pool.map(decode, files))
    elapsed = time.perf_counter() - start
    # Full-memory checksums are outside the timer. Bounded batches include the
    # checksum consumer so no earlier arrays need to stay in memory.
    signatures = outputs if batches else [_event_signature(events) for events in outputs]
    return {"seconds": elapsed, "signature": signatures,
            "event_count": sum(signature["count"] for signature in signatures)}


def _python_feed(evt3, path, chunk_bytes, **options):
    # Prepare the same recorded 8-MiB payload as the native packet benchmark.
    # Header parsing, input slicing and imports are outside the decode timer.
    with Path(path).open("rb") as stream:
        while stream.peek(1)[:1] == b"%":
            if stream.readline().startswith(b"% end"):
                break
        data = stream.read(8 << 20)
    data = data[:len(data) & ~1]
    chunks = [data[start:start + chunk_bytes] for start in range(0, len(data), chunk_bytes)]
    decoder = evt3.Decoder(**options)
    outputs = []
    latencies = []
    gc.collect()
    start = time.perf_counter()
    for chunk in chunks:
        packet_start = time.perf_counter()
        outputs.append(decoder.feed(chunk))
        latencies.append(time.perf_counter() - packet_start)
    decoder.finish()
    elapsed = time.perf_counter() - start
    # Arrays remain alive throughout timing; checksums are computed afterwards.
    signature = {"count": 0, "sums": [0, 0, 0, 0],
                 "first_timestamp": None, "last_timestamp": None}
    for events in outputs:
        part = _event_signature(events)
        if part["count"]:
            if signature["first_timestamp"] is None:
                signature["first_timestamp"] = part["first_timestamp"]
            signature["last_timestamp"] = part["last_timestamp"]
        signature["count"] += part["count"]
        signature["sums"] = [(a + b) & ((1 << 64) - 1)
                             for a, b in zip(signature["sums"], part["sums"])]
    ordered = sorted(latencies)
    import math
    percentile = lambda p: ordered[math.ceil((len(ordered) - 1) * p)] if ordered else 0
    return {"seconds": elapsed, "signature": [signature], "event_count": signature["count"],
            "packet_latency_seconds": {"count": len(ordered), "p50": percentile(.5),
                                       "p95": percentile(.95), "p99": percentile(.99),
                                       "max": percentile(1)}}


def run_python(package_dir, files, workers, batches=False, feed_chunk_bytes=None, threads=None):
    request = {"package_dir": str(package_dir), "files": [str(path) for path in files],
               "workers": workers, "batches": batches, "feed_chunk_bytes": feed_chunk_bytes, "threads": threads}
    output = checked([sys.executable, str(Path(__file__).resolve()), json.dumps(request)])
    return json.loads(output)


def run_native(binary, arguments, threads=None):
    option = [] if threads is None else ["--threads", str(threads)]
    return json.loads(checked([str(binary), *option, *arguments]))


def build_rust(source, target, destination, configurable=False):
    env = dict(os.environ, PYO3_PYTHON=sys.executable, CARGO_TARGET_DIR=str(target))
    checked(["cargo", "build", "--release", "--locked", "--manifest-path",
             str(source / "Cargo.toml"), "-p", "evt3-cli"], env=env)
    binary = destination / ("evt3.exe" if os.name == "nt" else "evt3")
    shutil.copy2(target / "release" / binary.name, binary)
    features = ["--features", "parallel"] if configurable else []
    checked(["cargo", "build", "--release", "--locked", "--manifest-path",
             str(source / "Cargo.toml"), "-p", "evt3", "--lib", *features], env=env)
    adapter = ["--cfg", "evt3_parallel_api"] if configurable else []
    checked(["rustc", "--edition=2021", "-O", "-C", "lto=thin", "-C", "codegen-units=1",
             str(ROOT / "benchmarks" / "rust_decode_worker.rs"), "--extern",
             f"evt3={target / 'release' / 'libevt3.rlib'}", "-L",
             f"dependency={target / 'release' / 'deps'}", "-o",
             str(destination / "native-worker"), *adapter], env=env)
    package_dir = destination / "python"
    wheels = destination / "wheels"
    checked([sys.executable, "-m", "maturin", "build", "--release", "--locked",
             "--manifest-path", str(source / "evt3-python" / "Cargo.toml"),
             "--out", str(wheels), "--interpreter", sys.executable], env=env)
    built = list(wheels.glob("*.whl"))
    if len(built) != 1:
        raise RuntimeError(f"Expected one Python wheel in {wheels}, found {len(built)}")
    with zipfile.ZipFile(built[0]) as wheel:
        wheel.extractall(package_dir)
    return binary, package_dir


def prepare_versions(args, directory):
    commit = checked(["git", "rev-parse", "--verify", f"{args.rust_baseline_ref}^{{commit}}"],
                     cwd=ROOT).strip()
    baseline = directory / "baseline-source"
    archive = subprocess.run(["git", "archive", "--format=tar", commit], cwd=ROOT,
                             check=True, capture_output=True).stdout
    with tarfile.open(fileobj=io.BytesIO(archive)) as contents:
        contents.extractall(baseline)
    lock = ROOT / "Cargo.lock"
    if not lock.exists():
        checked(["cargo", "generate-lockfile", "--manifest-path", str(ROOT / "Cargo.toml")])
    shutil.copy2(lock, baseline / "Cargo.lock")
    # Refresh only workspace dependency edges when the candidate adds a
    # dependency. Existing resolved external versions stay pinned.
    checked(["cargo", "update", "--offline", "--workspace", "--manifest-path",
             str(baseline / "Cargo.toml")])

    versions = {}
    python_dirs = {}
    for name, source, target in [
        ("rust_before", baseline, args.work_dir / "baseline-target"),
        ("rust_current", ROOT, args.work_dir / "current-target"),
    ]:
        print(f"Building {name} ({commit if name == 'rust_before' else 'working tree'})...",
              flush=True)
        destination = directory / name
        destination.mkdir()
        versions[name], python_dirs[name] = build_rust(source, target, destination, configurable=name == "rust_current")

    cpp_source = ROOT / "cpp_reference" / "metavision_evt3_raw_file_decoder.cpp"
    cxx = os.environ.get("CXX", "c++")
    flags = ["-O3", "-DNDEBUG", "-std=c++17"]
    if args.cpp_binary:
        versions["prophesee_cpp"] = args.cpp_binary.resolve()
        cpp_build = {"provided_binary": str(versions["prophesee_cpp"]), "flags": None}
    else:
        print("Building Prophesee standalone C++ reference (-O3 -DNDEBUG)...", flush=True)
        versions["prophesee_cpp"] = directory / (
            "prophesee-cpp.exe" if os.name == "nt" else "prophesee-cpp"
        )
        checked([cxx, *flags, str(cpp_source), "-o", str(versions["prophesee_cpp"])])
        cpp_build = {"compiler": checked([cxx, "--version"]).splitlines()[0], "flags": flags}
    for binary in versions.values():
        if not binary.is_file():
            raise FileNotFoundError(binary)
    provenance = {
        "rust_baseline_ref": args.rust_baseline_ref, "rust_baseline_commit": commit,
        "rust_current_commit": checked(["git", "rev-parse", "HEAD"], cwd=ROOT).strip(),
        "rust_current_diff_sha256": hashlib.sha256(
            checked(["git", "diff", "HEAD", "--", "evt3-core", "evt3-cli", "evt3-python"],
                    cwd=ROOT).encode()).hexdigest(),
        "rustc": checked(["rustc", "-Vv"]).strip(), "python": sys.version,
        "dependency_lock_sha256": sha256(lock),
        "baseline_dependency_lock_sha256": sha256(baseline / "Cargo.lock"),
        "current_source_sha256": {
            str(path.relative_to(ROOT)): sha256(path)
            for path in [ROOT / "Cargo.toml", *[
                path for crate in ("evt3-core", "evt3-cli", "evt3-python")
                for path in [ROOT / crate / "Cargo.toml", *sorted((ROOT / crate / "src").rglob("*.rs"))]
            ], *sorted((ROOT / "evt3-python" / "python").rglob("*.py"))]
        },
        "rust_flags": {key: os.environ.get(key)
                       for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")},
        "platform": platform.platform(), "logical_cpu_count": os.cpu_count(),
        "native_harness_flags": ["-O", "-C", "lto=thin", "-C", "codegen-units=1"],
        "native_candidate_adapter_cfg": "evt3_parallel_api",
        "candidate_parallel_feature": True,
        "candidate_decoder_threads": args.decoder_threads,
        "candidate_defaults": {"rust_library": "Off", "python": "Auto", "cli": "Auto"},
        "cpp_source": str(cpp_source),
        "cpp_source_url": CPP_URL, "cpp_source_sha256": sha256(cpp_source),
        "cpp_build": cpp_build,
        "binary_sha256": {name: sha256(binary) for name, binary in versions.items()},
        "native_worker_source_sha256": sha256(ROOT / "benchmarks" / "rust_decode_worker.rs"),
        "native_worker_binary_sha256": {name: sha256(path.parent / "native-worker")
                                        for name, path in python_dirs.items()},
        "python_wheel_sha256": {name: sha256(next((path.parent / "wheels").glob("*.whl")))
                                for name, path in python_dirs.items()},
        "work_directory": str(args.work_dir),
    }
    return versions, python_dirs, provenance


def run_suite(args):
    if args.decoder_threads is not None and args.decoder_threads < 0:
        raise ValueError("Decoder threads must be non-negative")
    if args.workers < 1 or args.small_mib < 1 or args.multi_file_mib < 1:
        raise ValueError("Workers and fixture sizes must be positive")
    args.work_dir = args.work_dir.resolve()
    args.work_dir.mkdir(parents=True, exist_ok=True)
    save_report({"status": "running", "phase": "building", "cases": []}, args.output)

    def selected(name):
        return not args.cases or any(fnmatch.fnmatchcase(name, pattern) for pattern in args.cases)

    with tempfile.TemporaryDirectory(prefix="comparison-", dir=args.work_dir) as temporary:
        directory = Path(temporary)
        binaries, python_dirs, provenance = prepare_versions(args, directory)
        small = directory / "small.raw"
        copy_prefix(args.file, small, args.small_mib << 20)
        if args.multi_files:
            many = [path.resolve() for path in args.multi_files]
        else:
            many = [directory / f"multi-{index}.raw" for index in range(4)]
            for path in many:
                copy_prefix(args.file, path, args.multi_file_mib << 20)
        for path in many:
            if not path.is_file():
                raise FileNotFoundError(path)
        inputs = {"small": [small], "large": [args.file.resolve()], "many": many}
        print("Verifying CSV equality against both baselines...", flush=True)
        verification = verify_csv(
            list(dict.fromkeys(path for files in inputs.values() for path in files)),
            binaries, directory, args.verify_prefix_mib << 20, threads=args.decoder_threads)
        report = {
            "status": "running", "provenance": provenance, "iterations": args.iterations,
            "case_selection": args.cases,
            "method": {"warmups_per_case_and_implementation": 1, "order": "rotating",
                       "filesystem_cache": "warm", "csv_output": os.devnull,
                       "csv_timing": "process startup + file read + decode + CSV formatting",
                       "python_timing": "file read + decode + array/object creation; imports and checksums excluded",
                       "python_batches_timing": "bounded decode + checksum consumer",
                       "native_timing": "file read + decode + output allocation; checksums excluded",
                       "native_stream_timing": "recorded payload in memory, reusable outputs, decode and packet clocks; verification excluded",
                       "python_feed_timing": "recorded payload in memory, Decoder.feed and NumPy/object allocation; checksums excluded",
                       "multi_input_origin": "provided files" if args.multi_files else "four identical recording prefixes",
                       "workers": args.workers},
            "inputs": {name: [{"path": str(path), "bytes": path.stat().st_size,
                                "sha256": sha256(path)} for path in files]
                       for name, files in inputs.items()},
            "csv_verification": verification, "cases": [],
            "unavailable": {"prophesee_cpp_python_decode_only":
                            "The unmodified standalone reference exports CSV, not NumPy arrays; no comparable decode-only ratio is reported."},
        }
        save_report(report, args.output)

        def record(result):
            if not result["name"].startswith("csv/"):
                reduction = result["current_time_reduction_vs"]["rust_before"]
                result["runtime_target"] = {"baseline": "rust_before", "maximum_time_ratio": .60,
                                            "minimum_time_reduction": .40, "passed": reduction >= .40}
            report["cases"].append(result)
            save_report(report, args.output)

        workloads = [("small", 1), ("large", 1), ("many_sequential", 1),
                     ("many_parallel", args.workers)]
        for workload, workers in ([] if args.decode_only else workloads):
            if not selected(f"csv/{workload}"):
                continue
            files = inputs["many" if workload.startswith("many") else workload]
            runners = {
                name: (lambda binary=binary, name=name: {
                    "seconds": run_csv(binary, files, workers, name != "prophesee_cpp",
                                       threads=args.decoder_threads if name == "rust_current" else None)})
                for name, binary in binaries.items()
            }
            result = compare_case(f"csv/{workload}", runners, args.iterations)
            result.update({"workers": workers,
                           "input_bytes": sum(path.stat().st_size for path in files)})
            record(result)

        native = {name: package.parent / "native-worker" for name, package in python_dirs.items()}
        for layout in ["rows", "columns"]:
            for workload, workers in workloads:
                if not selected(f"rust_{layout}/{workload}"):
                    continue
                files = inputs["many" if workload.startswith("many") else workload]
                runners = {name: (lambda binary=binary, name=name: run_native(
                    binary, [layout, str(workers), *map(str, files)],
                    threads=args.decoder_threads if name == "rust_current" else None))
                    for name, binary in native.items()}
                result = compare_case(f"rust_{layout}/{workload}", runners, args.iterations)
                result.update({"workers": workers, "input_bytes": sum(path.stat().st_size for path in files)})
                record(result)

        for chunk_bytes in [4095, 8192, 65536]:
            for layout in ["rows", "columns"]:
                if not selected(f"rust_stream_{layout}/{chunk_bytes}"):
                    continue
                runners = {name: (lambda binary=binary, name=name: run_native(
                    binary, [f"stream_{layout}", str(chunk_bytes), str(args.file.resolve())],
                    threads=args.decoder_threads if name == "rust_current" else None))
                    for name, binary in native.items()}
                record(compare_case(f"rust_stream_{layout}/{chunk_bytes}", runners, args.iterations))
            if selected(f"python_feed/{chunk_bytes}"):
                runners = {name: (lambda package=package, name=name: run_python(
                    package, inputs["large"], 1, feed_chunk_bytes=chunk_bytes,
                    threads=args.decoder_threads if name == "rust_current" else None))
                    for name, package in python_dirs.items()}
                record(compare_case(f"python_feed/{chunk_bytes}", runners, args.iterations))

        for workload, workers in workloads:
            if not selected(f"python_full/{workload}"):
                continue
            files = inputs["many" if workload.startswith("many") else workload]
            runners = {
                name: (lambda package_dir=package_dir, name=name: run_python(package_dir, files, workers,
                    threads=args.decoder_threads if name == "rust_current" else None))
                for name, package_dir in python_dirs.items()
            }
            result = compare_case(f"python_full/{workload}", runners, args.iterations)
            result.update({"workers": workers, "input_bytes": sum(path.stat().st_size for path in files)})
            record(result)

        runners = {
            name: (lambda package_dir=package_dir, name=name: run_python(package_dir, inputs["large"], 1, True,
                threads=args.decoder_threads if name == "rust_current" else None))
            for name, package_dir in python_dirs.items()
        }
        if selected("python_batches/large"):
            record(compare_case("python_batches/large", runners, args.iterations))
        if not report["cases"]:
            raise ValueError("No comparison cases matched the selection")
        report["status"] = "complete"
        report.update(runtime_acceptance(report["cases"]))
        save_report(report, args.output)
        print("\nMedian seconds; speedup = baseline time / current Rust time")
        print(f"{'Workload':<29} {'C++':>9} {'Rust before':>12} {'Current':>10} {'vs C++':>9} {'vs before':>10}")
        for case in report["cases"]:
            results = case["implementations"]
            value = lambda name: f"{results[name]['median_seconds']:.4f}" if name in results else "n/a"
            ratios = case["current_speedup_vs"]
            cpp = f"{ratios['prophesee_cpp']:.3f}x" if "prophesee_cpp" in ratios else "n/a"
            print(f"{case['name']:<29} {value('prophesee_cpp'):>9} {value('rust_before'):>12} "
                  f"{value('rust_current'):>10} {cpp:>9} {ratios['rust_before']:>9.3f}x")
        return report


def runtime_acceptance(cases):
    """Keep a passing selection from being mistaken for full acceptance."""
    files = ["small", "large", "many_sequential", "many_parallel"]
    required = {f"{family}/{workload}"
                for family in ("rust_rows", "rust_columns", "python_full")
                for workload in files}
    required.update(f"{family}/{size}"
                    for family in ("rust_stream_rows", "rust_stream_columns", "python_feed")
                    for size in (4095, 8192, 65536))
    required.add("python_batches/large")
    targets = {case["name"]: case["runtime_target"]["passed"]
               for case in cases if "runtime_target" in case}
    missing = sorted(required - targets.keys())
    passed = bool(targets) and all(targets.values())
    return {"all_runtime_targets_passed": passed,
            "missing_runtime_cases": missing,
            "full_runtime_acceptance_passed": not missing and passed}


if __name__ == "__main__":
    print(json.dumps(python_worker(json.loads(sys.argv[1]))))
