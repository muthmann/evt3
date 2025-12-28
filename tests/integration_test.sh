#!/bin/bash
# Integration test comparing Rust decoder output against C++ reference implementation
# Run from repository root: ./tests/integration_test.sh

set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
ROOT_DIR="$( cd "$SCRIPT_DIR/.." && pwd )"
TEST_DATA_DIR="$ROOT_DIR/test_data"
TEMP_DIR=$(mktemp -d)

# Cleanup on exit
trap "rm -rf $TEMP_DIR" EXIT

echo "=== EVT3 Decoder Integration Test ==="
echo ""

# Check if test file exists
TEST_FILE="$TEST_DATA_DIR/laser.raw"
if [ ! -f "$TEST_FILE" ]; then
    echo "ERROR: Test file not found: $TEST_FILE"
    echo "Download a sample EVT3 file from Prophesee's datasets"
    exit 1
fi

# Check if Rust decoder exists
RUST_DECODER="$ROOT_DIR/target/release/evt3"
if [ ! -f "$RUST_DECODER" ]; then
    echo "Building Rust decoder..."
    cargo build --release -p evt3-cli --manifest-path "$ROOT_DIR/Cargo.toml"
fi

# Check if C++ decoder exists
CPP_DECODER="$ROOT_DIR/cpp_reference/evt3_decoder"
if [ ! -f "$CPP_DECODER" ]; then
    echo "Building C++ reference decoder..."
    mkdir -p "$ROOT_DIR/cpp_reference"
    curl -sL "https://raw.githubusercontent.com/prophesee-ai/openeb/main/standalone_samples/metavision_evt3_raw_file_decoder/metavision_evt3_raw_file_decoder.cpp" \
        -o "$ROOT_DIR/cpp_reference/metavision_evt3_raw_file_decoder.cpp"
    g++ -std=c++17 -O2 -o "$CPP_DECODER" "$ROOT_DIR/cpp_reference/metavision_evt3_raw_file_decoder.cpp"
fi

OUTPUT_RUST="$TEMP_DIR/output_rust.csv"
OUTPUT_CPP="$TEMP_DIR/output_cpp.csv"

echo "Running Rust decoder..."
START_RUST=$(python3 -c 'import time; print(time.time())')
"$RUST_DECODER" "$TEST_FILE" "$OUTPUT_RUST" --quiet
END_RUST=$(python3 -c 'import time; print(time.time())')
RUST_TIME=$(python3 -c "print(f'{$END_RUST - $START_RUST:.3f}')")

echo "Running C++ decoder..."
START_CPP=$(python3 -c 'import time; print(time.time())')
"$CPP_DECODER" "$TEST_FILE" "$OUTPUT_CPP" 2>/dev/null
END_CPP=$(python3 -c 'import time; print(time.time())')
CPP_TIME=$(python3 -c "print(f'{$END_CPP - $START_CPP:.3f}')")

echo ""
echo "=== Results ==="

# Count events
RUST_COUNT=$(wc -l < "$OUTPUT_RUST" | tr -d ' ')
CPP_COUNT=$(wc -l < "$OUTPUT_CPP" | tr -d ' ')
echo "Rust events:  $RUST_COUNT"
echo "C++ events:   $CPP_COUNT"

# Compare MD5 checksums
RUST_MD5=$(md5 -q "$OUTPUT_RUST" 2>/dev/null || md5sum "$OUTPUT_RUST" | cut -d' ' -f1)
CPP_MD5=$(md5 -q "$OUTPUT_CPP" 2>/dev/null || md5sum "$OUTPUT_CPP" | cut -d' ' -f1)
echo "Rust MD5:     $RUST_MD5"
echo "C++ MD5:      $CPP_MD5"

echo ""
echo "=== Performance ==="
echo "Rust time:    ${RUST_TIME}s"
echo "C++ time:     ${CPP_TIME}s"

echo ""
if [ "$RUST_MD5" = "$CPP_MD5" ]; then
    echo "✅ TEST PASSED: Output matches C++ reference implementation!"
    EVENTS=$((RUST_COUNT - 1))  # Subtract header line
    SPEEDUP=$(python3 -c "print(f'{$CPP_TIME / $RUST_TIME:.2f}x' if $RUST_TIME > 0 else 'N/A')")
    echo "   Decoded $EVENTS events"
    echo "   Rust is ${SPEEDUP} faster than C++"
    exit 0
else
    echo "❌ TEST FAILED: Output differs from C++ reference!"
    echo "First difference:"
    diff "$OUTPUT_RUST" "$OUTPUT_CPP" | head -20
    exit 1
fi
