"""Free-threaded CPython compatibility tests."""

import os
import sys
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest


REQUIRE_FREE_THREADED = os.environ.get("EVT3_REQUIRE_FREE_THREADED") == "1"
GIL_CHECK = getattr(sys, "_is_gil_enabled", None)
WAS_FREE_THREADED = GIL_CHECK is not None and not GIL_CHECK()


@pytest.fixture(autouse=True)
def require_free_threaded_runtime():
    """Run these tests only with a free-threaded interpreter."""
    if REQUIRE_FREE_THREADED:
        assert WAS_FREE_THREADED, "CI did not start a free-threaded Python build"
    elif not WAS_FREE_THREADED:
        pytest.skip("requires free-threaded CPython")


def snapshot(events):
    """Convert decoded arrays to immutable values for thread-safe comparison."""
    return (
        tuple(events.x.tolist()),
        tuple(events.y.tolist()),
        tuple(events.p.tolist()),
        tuple(events.t.tolist()),
    )


def test_import_keeps_gil_disabled():
    """Importing the extension must not silently enable the GIL."""
    import evt3  # noqa: F401

    assert not sys._is_gil_enabled()


def test_independent_decoders_work_in_parallel(synthetic_evt3_bytes):
    """Independent stateless and stateful decoders can run concurrently."""
    import evt3

    worker_count = 8
    barrier = threading.Barrier(worker_count)
    expected = snapshot(evt3.decode_bytes(synthetic_evt3_bytes))

    def decode_once(_worker_id):
        barrier.wait(timeout=10)

        stateless = snapshot(evt3.decode_bytes(synthetic_evt3_bytes))
        decoder = evt3.Decoder()
        stateful = snapshot(decoder.feed(synthetic_evt3_bytes))
        decoder.finish()
        return stateless, stateful

    with ThreadPoolExecutor(max_workers=worker_count) as executor:
        results = list(executor.map(decode_once, range(worker_count)))

    assert results == [(expected, expected)] * worker_count


def test_shared_events_support_parallel_reads(synthetic_evt3_bytes):
    """A decoded event container supports concurrent read-only access."""
    import evt3

    worker_count = 8
    barrier = threading.Barrier(worker_count)
    events = evt3.decode_bytes(synthetic_evt3_bytes)
    expected = snapshot(events)

    def read_once(_worker_id):
        barrier.wait(timeout=10)
        return snapshot(events)

    with ThreadPoolExecutor(max_workers=worker_count) as executor:
        results = list(executor.map(read_once, range(worker_count)))

    assert results == [expected] * worker_count
