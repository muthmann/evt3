//! Integration tests for EVT3 decoder using real recorded data.
//!
//! These tests require the test_data directory to contain sample EVT3 files.
//! Run with: cargo test --test integration_tests

use evt3_core::{output, Evt3Decoder, FieldOrder};
use std::path::Path;

const TEST_FILE: &str = "test_data/laser.raw";

/// Test that the decoder can successfully decode a real EVT3 file.
#[test]
fn test_decode_real_file() {
    let test_path = Path::new(TEST_FILE);
    if !test_path.exists() {
        eprintln!("Skipping test: test file not found at {}", TEST_FILE);
        return;
    }

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(test_path)
        .expect("Failed to decode file");

    // Verify metadata was parsed correctly
    assert_eq!(result.metadata.width, 1280);
    assert_eq!(result.metadata.height, 720);

    // Verify we got a reasonable number of events (laser.raw has ~116M events)
    assert!(
        result.cd_events.len() > 100_000_000,
        "Expected >100M events, got {}",
        result.cd_events.len()
    );

    // Verify first event structure is valid
    let first_event = &result.cd_events[0];
    assert!(first_event.x < 1280);
    assert!(first_event.y < 720);
    assert!(first_event.polarity <= 1);
}

/// Test that timestamps are monotonically increasing (accounting for loops).
#[test]
fn test_timestamps_monotonic() {
    let test_path = Path::new(TEST_FILE);
    if !test_path.exists() {
        eprintln!("Skipping test: test file not found at {}", TEST_FILE);
        return;
    }

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(test_path)
        .expect("Failed to decode file");

    // Check that timestamps are non-decreasing
    // (Note: multiple events can have the same timestamp)
    let mut last_time = 0u64;
    for (i, event) in result.cd_events.iter().enumerate() {
        assert!(
            event.timestamp >= last_time,
            "Timestamp decreased at event {}: {} -> {}",
            i,
            last_time,
            event.timestamp
        );
        last_time = event.timestamp;
    }
}

/// Test that all coordinates are within sensor bounds.
#[test]
fn test_coordinates_in_bounds() {
    let test_path = Path::new(TEST_FILE);
    if !test_path.exists() {
        eprintln!("Skipping test: test file not found at {}", TEST_FILE);
        return;
    }

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(test_path)
        .expect("Failed to decode file");

    for (i, event) in result.cd_events.iter().enumerate() {
        assert!(
            event.x < result.metadata.width as u16,
            "Event {} x={} exceeds width {}",
            i,
            event.x,
            result.metadata.width
        );
        assert!(
            event.y < result.metadata.height as u16,
            "Event {} y={} exceeds height {}",
            i,
            event.y,
            result.metadata.height
        );
        assert!(
            event.polarity <= 1,
            "Event {} has invalid polarity {}",
            i,
            event.polarity
        );
    }
}

/// Test different field order outputs.
#[test]
fn test_field_order_formats() {
    let test_path = Path::new(TEST_FILE);
    if !test_path.exists() {
        eprintln!("Skipping test: test file not found at {}", TEST_FILE);
        return;
    }

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(test_path)
        .expect("Failed to decode file");

    // Take first 10 events for comparison
    let events: Vec<_> = result.cd_events.iter().take(10).cloned().collect();

    // Test XYPT (default)
    let mut output_xypt = Vec::new();
    {
        let mut writer = output::CsvWriter::new(&mut output_xypt, FieldOrder::XYPT);
        writer.write_events(&events).unwrap();
        writer.flush().unwrap();
    }
    let xypt_str = String::from_utf8(output_xypt).unwrap();
    // Check format is correct (x,y,p,t)
    assert!(xypt_str.lines().nth(1).unwrap().split(',').count() == 4);

    // Test TXYP
    let mut output_txyp = Vec::new();
    {
        let mut writer = output::CsvWriter::new(&mut output_txyp, FieldOrder::TXYP);
        writer.write_events(&events).unwrap();
        writer.flush().unwrap();
    }
    let txyp_str = String::from_utf8(output_txyp).unwrap();
    assert!(txyp_str.lines().nth(1).unwrap().split(',').count() == 4);
}

/// Test binary output format.
#[test]
fn test_binary_output() {
    let test_path = Path::new(TEST_FILE);
    if !test_path.exists() {
        eprintln!("Skipping test: test file not found at {}", TEST_FILE);
        return;
    }

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(test_path)
        .expect("Failed to decode file");

    // Write to binary
    let temp_path = std::env::temp_dir().join("evt3_test_output.bin");
    output::write_binary(&temp_path, &result.cd_events, &result.metadata).unwrap();

    // Verify header
    let data = std::fs::read(&temp_path).unwrap();
    assert_eq!(&data[0..8], b"EVT3BIN\0");

    // Version
    let version = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    assert_eq!(version, 1);

    // Width/Height
    let width = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let height = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    assert_eq!(width, 1280);
    assert_eq!(height, 720);

    // Event count
    let count = u64::from_le_bytes([
        data[20], data[21], data[22], data[23], data[24], data[25], data[26], data[27],
    ]);
    assert_eq!(count, result.cd_events.len() as u64);

    // Cleanup
    std::fs::remove_file(&temp_path).ok();
}

/// Benchmark-style test to measure throughput.
#[test]
fn test_decode_performance() {
    let test_path = Path::new(TEST_FILE);
    if !test_path.exists() {
        eprintln!("Skipping test: test file not found at {}", TEST_FILE);
        return;
    }

    let start = std::time::Instant::now();

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(test_path)
        .expect("Failed to decode file");

    let duration = start.elapsed();
    let events_per_sec = result.cd_events.len() as f64 / duration.as_secs_f64();

    eprintln!(
        "Performance: decoded {} events in {:.2}s ({:.0} events/s)",
        result.cd_events.len(),
        duration.as_secs_f64(),
        events_per_sec
    );

    // Assert minimum performance threshold (5M events/s)
    assert!(
        events_per_sec > 5_000_000.0,
        "Performance too slow: {:.0} events/s (expected >5M)",
        events_per_sec
    );
}
