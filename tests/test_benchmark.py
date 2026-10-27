"""Regression tests for benchmark fairness, validation, and workload handling."""

import importlib.util
import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch


BENCHMARKS = Path(__file__).resolve().parents[1] / "benchmarks"


def load_module(name):
    spec = importlib.util.spec_from_file_location(name, BENCHMARKS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


suite = load_module("comparison_suite")
legacy = load_module("benchmark")


class BenchmarkTests(unittest.TestCase):
    def test_native_runner_only_adds_an_explicit_thread_option_when_requested(self):
        with patch.object(suite, "checked", return_value='{"seconds": 1}') as run:
            suite.run_native(Path("worker"), ["rows", "1", "file.raw"], threads=1)
            self.assertEqual(run.call_args.args[0],
                             ["worker", "--threads", "1", "rows", "1", "file.raw"])
            suite.run_native(Path("worker"), ["rows", "1", "file.raw"])
            self.assertEqual(run.call_args.args[0], ["worker", "rows", "1", "file.raw"])

    def test_python_worker_request_records_an_explicit_serial_setting(self):
        with patch.object(suite, "checked", return_value='{"seconds": 1}') as run:
            suite.run_python(Path("package"), [Path("file.raw")], 4, threads=1)
            request = json.loads(run.call_args.args[0][-1])
            self.assertEqual(request["threads"], 1)
            self.assertEqual(request["workers"], 4)

    def test_prefix_preserves_header_and_complete_words(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.raw"
            target = Path(directory) / "prefix.raw"
            header = b"% geometry 1280x720\n% end\n"
            payload = bytes.fromhex("0080646028000128")
            source.write_bytes(header + payload)
            suite.copy_prefix(source, target, len(header) + 7)
            self.assertEqual(target.read_bytes(), header + payload[:6])
            suite.copy_prefix(source, target, 1 << 20)
            self.assertEqual(target.read_bytes(), source.read_bytes())

    def test_report_marks_incomplete_runs_and_replaces_the_snapshot(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "results.json"
            suite.save_report({"status": "running", "cases": []}, output)
            self.assertEqual(json.loads(output.read_text())["status"], "running")
            suite.save_report({"status": "complete", "cases": [1]}, output)
            self.assertEqual(json.loads(output.read_text()),
                             {"status": "complete", "cases": [1]})
            self.assertFalse(output.with_suffix(".json.tmp").exists())

    def test_headerless_prefix_and_odd_tail(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.raw"
            target = Path(directory) / "prefix.raw"
            source.write_bytes(bytes.fromhex("008001280a"))
            suite.copy_prefix(source, target, 100)
            self.assertEqual(target.read_bytes(), bytes.fromhex("00800128"))

    def test_prefix_rejects_header_without_payload(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.raw"
            source.write_bytes(b"% end\n")
            with self.assertRaises(ValueError):
                suite.copy_prefix(source, Path(directory) / "prefix.raw", 6)

    def test_csv_verification_rejects_a_wrong_decoder(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            source = directory / "source.raw"
            source.write_bytes(bytes.fromhex("00800128"))

            def fake_decode(binary, files, workers, rust, output):
                Path(output).write_bytes(b"wrong\n" if binary == "bad" else b"correct\n")

            with patch.object(suite, "run_csv", side_effect=fake_decode):
                with self.assertRaisesRegex(RuntimeError, "CSV output differs"):
                    suite.verify_csv([source], {"prophesee_cpp": "ok", "rust_current": "bad"},
                                     directory, 100)

    def test_case_rotates_all_three_implementations_and_uses_medians(self):
        calls = []

        def runner(label, seconds):
            def run():
                calls.append(label)
                return {"seconds": seconds}
            return run

        runners = {"rust_before": runner("rust_before", 2.0),
                   "rust_current": runner("rust_current", 1.0),
                   "prophesee_cpp": runner("prophesee_cpp", 3.0)}
        with redirect_stdout(io.StringIO()):
            result = suite.compare_case("csv/test", runners, 3)
        self.assertEqual(calls[:3], list(runners))  # Untimed warm-up per implementation.
        self.assertEqual(calls[3:6], list(runners))
        self.assertEqual(calls[6:9], ["rust_current", "prophesee_cpp", "rust_before"])
        self.assertEqual(result["current_speedup_vs"],
                         {"rust_before": 2.0, "prophesee_cpp": 3.0})
        self.assertEqual(result["implementations"]["rust_current"]["samples_seconds"],
                         [1.0, 1.0, 1.0])

    def test_python_comparison_fails_when_event_signatures_differ(self):
        runners = {
            "rust_before": lambda: {"seconds": 1, "event_count": 1, "signature": [1]},
            "rust_current": lambda: {"seconds": 1, "event_count": 1, "signature": [2]},
        }
        with redirect_stdout(io.StringIO()):
            with self.assertRaisesRegex(RuntimeError, "Decoded events differ"):
                suite.compare_case("python/test", runners, 1)

    def test_throughput_increase_is_converted_to_runtime_reduction(self):
        runners = {"rust_before": lambda: {"seconds": 1.4},
                   "rust_current": lambda: {"seconds": 1.0}}
        with redirect_stdout(io.StringIO()):
            result = suite.compare_case("runtime/test", runners, 1)
        self.assertAlmostEqual(result["current_time_reduction_vs"]["rust_before"], 2 / 7)
        self.assertLess(result["current_time_reduction_vs"]["rust_before"], .40)

    def test_packet_latency_samples_exclude_warmup(self):
        calls = {"rust_before": 0, "rust_current": 0}

        def runner(label):
            def run():
                calls[label] += 1
                return {"seconds": 1.0,
                        "packet_latency_seconds": {"count": 10, "p99": calls[label]}}
            return run

        with redirect_stdout(io.StringIO()):
            result = suite.compare_case("packets/test", {label: runner(label) for label in calls}, 3)
        for implementation in result["implementations"].values():
            self.assertEqual([sample["p99"] for sample in implementation["packet_latency_samples"]],
                             [2, 3, 4])

    def test_passing_case_selection_does_not_establish_full_acceptance(self):
        case = {"name": "python_full/large", "runtime_target": {"passed": True}}
        result = suite.runtime_acceptance([case])
        self.assertTrue(result["all_runtime_targets_passed"])
        self.assertFalse(result["full_runtime_acceptance_passed"])
        self.assertIn("rust_stream_rows/65536", result["missing_runtime_cases"])
        self.assertNotIn(case["name"], result["missing_runtime_cases"])

    def test_full_acceptance_requires_every_runtime_case_and_a_nonempty_selection(self):
        empty = suite.runtime_acceptance([])
        self.assertFalse(empty["all_runtime_targets_passed"])
        cases = [{"name": name, "runtime_target": {"passed": True}}
                 for name in empty["missing_runtime_cases"]]
        self.assertTrue(suite.runtime_acceptance(cases)["full_runtime_acceptance_passed"])
        cases[0]["runtime_target"]["passed"] = False
        self.assertFalse(suite.runtime_acceptance(cases)["full_runtime_acceptance_passed"])

    def test_python_feed_prepares_payload_and_preserves_odd_chunk_boundaries(self):
        fed = []
        finished = []

        class Decoder:
            def feed(self, chunk):
                fed.append(chunk)
                return len(fed)

            def finish(self):
                finished.append(True)

        class FakeModule:
            pass

        FakeModule.Decoder = Decoder
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "packets.raw"
            payload = bytes.fromhex("00800128022803280428")
            path.write_bytes(b"% geometry 1280x720\n% end\n" + payload + b"\n")
            signature = lambda index: {"count": 1, "sums": [index] * 4,
                                       "first_timestamp": index, "last_timestamp": index}
            with patch.object(suite, "_event_signature", side_effect=signature):
                result = suite._python_feed(FakeModule, path, 3)
        self.assertEqual(b"".join(fed), payload)
        self.assertEqual([len(chunk) for chunk in fed], [3, 3, 3, 1])
        self.assertEqual(finished, [True])
        self.assertEqual(result["signature"], [{"count": 4, "sums": [10] * 4,
                                               "first_timestamp": 1, "last_timestamp": 4}])
        self.assertEqual(result["packet_latency_seconds"]["count"], 4)

    def test_failed_commands_are_not_recorded_as_fast_runs(self):
        with self.assertRaisesRegex(RuntimeError, "Command failed"):
            suite.checked([sys.executable, "-c", "raise SystemExit(2)"])

    def test_batch_signature_preserves_edges_and_wraps_sums(self):
        class Decoder:
            @staticmethod
            def decode_file_batches(path):
                return ["empty", "first", "last"]

        signatures = [
            {"count": 0, "sums": [0, 0, 0, 0],
             "first_timestamp": None, "last_timestamp": None},
            {"count": 1, "sums": [3, 4, 1, (1 << 64) - 1],
             "first_timestamp": 10, "last_timestamp": 10},
            {"count": 2, "sums": [7, 8, 1, 3],
             "first_timestamp": 20, "last_timestamp": 30},
        ]
        with patch.object(suite, "_event_signature", side_effect=signatures):
            result = suite._batch_signature(Decoder, "input.raw")
        self.assertEqual(result, {"count": 3, "sums": [10, 12, 2, 2],
                                  "first_timestamp": 10, "last_timestamp": 30})

    def test_multi_file_csv_waits_for_all_files_and_propagates_errors(self):
        calls = []

        def fake_checked(command):
            calls.append(command)
            if "bad.raw" in command:
                raise RuntimeError("bad input")

        with patch.object(suite, "checked", side_effect=fake_checked):
            suite.run_csv("decoder", ["a.raw", "b.raw"], 2, True)
            self.assertEqual(len(calls), 2)
            self.assertTrue(all(command[-1] == "--quiet" for command in calls))
            with self.assertRaisesRegex(RuntimeError, "bad input"):
                suite.run_csv("decoder", ["a.raw", "bad.raw"], 2, False)

    def test_legacy_summary_does_not_compare_python_decode_with_cpp_csv(self):
        results = [{"name": name, "avg_time": time, "event_count": 10,
                    "events_per_sec": 10 / time}
                   for name, time in [("Rust (Python)", 1), ("C++ Reference", 5)]]
        output = io.StringIO()
        with redirect_stdout(output):
            legacy.print_results(results, BENCHMARKS / "benchmark.py")
        self.assertNotIn("Speedup", output.getvalue())
        self.assertNotIn("5.00x", output.getvalue())
        self.assertIn("also format and write CSV", output.getvalue())


if __name__ == "__main__":
    unittest.main()
