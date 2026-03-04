//! Integration tests for EVT3 decoder using real recorded data.
//!
//! These tests require the test_data directory to contain sample EVT3 files.
//! Run with: cargo test --test integration_tests

use evt3_core::{output, Evt3Decoder, FieldOrder};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const TEST_FILE_CANDIDATES: [&str; 2] = ["test_data/laser.raw", "../test_data/laser.raw"];

fn test_file_path() -> Option<PathBuf> {
    TEST_FILE_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
}

fn open_payload_reader(test_path: &Path) -> (BufReader<File>, usize) {
    let file = File::open(test_path).expect("Failed to open test file");
    let mut reader = BufReader::new(file);

    loop {
        let bytes_peeked = reader.fill_buf().expect("Failed to peek test file");
        if bytes_peeked.is_empty() || bytes_peeked[0] != b'%' {
            break;
        }

        let mut line = String::new();
        reader
            .read_line(&mut line)
            .expect("Failed to read test file header");

        if line.starts_with("% end") {
            break;
        }
    }

    let payload_offset = reader
        .stream_position()
        .expect("Failed to determine payload offset") as usize;
    let file_size = std::fs::metadata(test_path)
        .expect("Failed to stat test file")
        .len() as usize;
    let mut payload_len = file_size - payload_offset;

    if payload_len % 2 == 1 {
        reader
            .seek(SeekFrom::End(-1))
            .expect("Failed to seek to payload tail");
        let mut tail = [0u8; 1];
        reader
            .read_exact(&mut tail)
            .expect("Failed to read payload tail");
        reader
            .seek(SeekFrom::Start(payload_offset as u64))
            .expect("Failed to rewind payload reader");

        if matches!(tail[0], b'\n' | b'\r') {
            payload_len -= 1;
        }
    }

    (reader, payload_len)
}

/// Test that the decoder can successfully decode a real EVT3 file.
#[test]
fn test_decode_real_file() {
    let Some(test_path) = test_file_path() else {
        eprintln!(
            "Skipping test: test file not found in {:?}",
            TEST_FILE_CANDIDATES
        );
        return;
    };

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(&test_path)
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
    let Some(test_path) = test_file_path() else {
        eprintln!(
            "Skipping test: test file not found in {:?}",
            TEST_FILE_CANDIDATES
        );
        return;
    };

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(&test_path)
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
    let Some(test_path) = test_file_path() else {
        eprintln!(
            "Skipping test: test file not found in {:?}",
            TEST_FILE_CANDIDATES
        );
        return;
    };

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(&test_path)
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
    let Some(test_path) = test_file_path() else {
        eprintln!(
            "Skipping test: test file not found in {:?}",
            TEST_FILE_CANDIDATES
        );
        return;
    };

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(&test_path)
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
    let Some(test_path) = test_file_path() else {
        eprintln!(
            "Skipping test: test file not found in {:?}",
            TEST_FILE_CANDIDATES
        );
        return;
    };

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(&test_path)
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
    let Some(test_path) = test_file_path() else {
        eprintln!(
            "Skipping test: test file not found in {:?}",
            TEST_FILE_CANDIDATES
        );
        return;
    };

    let start = std::time::Instant::now();

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(&test_path)
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

/// Test that chunked byte streaming matches decode_file on a real .raw file.
#[test]
fn test_decode_bytes_matches_decode_file_on_real_file() {
    let Some(test_path) = test_file_path() else {
        eprintln!(
            "Skipping test: test file not found in {:?}",
            TEST_FILE_CANDIDATES
        );
        return;
    };

    let mut baseline_decoder = Evt3Decoder::new();
    let baseline = baseline_decoder
        .decode_file(&test_path)
        .expect("Failed to decode baseline file");

    let (mut reader, mut payload_bytes_remaining) = open_payload_reader(&test_path);
    let mut streaming_decoder = Evt3Decoder::new();
    let mut streamed_cd_events = Vec::new();
    let mut streamed_trigger_events = Vec::new();
    let mut buffer = vec![0u8; 4095];
    let mut cd_offset = 0usize;
    let mut trigger_offset = 0usize;

    while payload_bytes_remaining > 0 {
        let chunk_len = buffer.len().min(payload_bytes_remaining);
        let bytes_read = reader
            .read(&mut buffer[..chunk_len])
            .expect("Failed to read payload");
        if bytes_read == 0 {
            break;
        }
        payload_bytes_remaining -= bytes_read;

        streaming_decoder
            .decode_bytes(
                &buffer[..bytes_read],
                &mut streamed_cd_events,
                &mut streamed_trigger_events,
            )
            .expect("Failed to stream-decode payload");

        let expected_cd = &baseline.cd_events[cd_offset..cd_offset + streamed_cd_events.len()];
        assert_eq!(streamed_cd_events, expected_cd);
        cd_offset += streamed_cd_events.len();
        streamed_cd_events.clear();

        let expected_triggers = &baseline.trigger_events
            [trigger_offset..trigger_offset + streamed_trigger_events.len()];
        assert_eq!(streamed_trigger_events, expected_triggers);
        trigger_offset += streamed_trigger_events.len();
        streamed_trigger_events.clear();
    }

    streaming_decoder
        .finish_stream()
        .expect("Streaming decoder ended on a dangling half-word");

    assert_eq!(cd_offset, baseline.cd_events.len());
    assert_eq!(trigger_offset, baseline.trigger_events.len());
}
