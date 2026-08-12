//! EVT 3.0 decoder CLI application.
//!
//! Decodes Prophesee EVT 3.0 raw files to various output formats.

use anyhow::{Context, Result};
use clap::Parser;
use evt3::output::{BinaryWriter, CsvWriter, TriggerCsvWriter};
use evt3::{ColumnarEventSink, EventFileReader, FieldOrder, SensorMetadata, DEFAULT_BATCH_BYTES};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

/// EVT 3.0 raw file decoder for Prophesee event cameras.
///
/// Decodes .raw files in EVT 3.0 format to human-readable CSV or efficient binary formats.
#[derive(Parser, Debug)]
#[command(name = "evt3-decode")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input EVT3 .raw file path
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// Output file path (.csv, .bin)
    ///
    /// The output format is determined by the file extension:
    /// - .csv: Comma-separated values (human-readable)
    /// - .bin: Binary format (efficient, for programmatic access)
    #[arg(value_name = "OUTPUT")]
    output: PathBuf,

    /// Field order for CSV output.
    ///
    /// Specify the order of fields in the output CSV.
    /// Format: comma-separated field names (x, y, p, t)
    ///
    /// Examples:
    /// - "x,y,p,t" (default, matches C++ reference)
    /// - "t,x,y,p" (timestamp first)
    /// - "x,y,t,p"
    #[arg(short, long, default_value = "x,y,p,t")]
    format: String,

    /// Output file for trigger events (optional)
    ///
    /// If provided, external trigger events will be written to this file.
    #[arg(short, long, value_name = "PATH")]
    triggers: Option<PathBuf>,

    /// Suppress progress output
    #[arg(short, long)]
    quiet: bool,
}

enum StreamingOutput {
    Csv(CsvWriter<File>),
    Binary(BinaryWriter<File>),
}

impl StreamingOutput {
    fn open(path: &Path, metadata: &SensorMetadata, field_order: FieldOrder) -> Result<Self> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("csv")
            .to_ascii_lowercase();
        let file = File::create(path).context("Failed to create output file")?;

        match extension.as_str() {
            "csv" => {
                let mut writer = CsvWriter::new(file, field_order);
                writer.write_header(Some(metadata))?;
                Ok(Self::Csv(writer))
            }
            "bin" => {
                let mut writer = BinaryWriter::new(file);
                writer.write_header(metadata, 0)?;
                Ok(Self::Binary(writer))
            }
            _ => anyhow::bail!("Unsupported output format: .{extension}. Use .csv or .bin"),
        }
    }

    fn write(&mut self, events: &evt3::EventColumns) -> Result<()> {
        match self {
            Self::Csv(writer) => writer.write_columns(events)?,
            Self::Binary(writer) => writer.write_columns(events)?,
        }
        Ok(())
    }

    fn finish(&mut self, event_count: u64) -> Result<()> {
        match self {
            Self::Csv(writer) => writer.flush()?,
            Self::Binary(writer) => {
                writer.update_event_count(event_count)?;
                writer.flush()?;
            }
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Parse field order
    let field_order = FieldOrder::from_str(&args.format)
        .context("Invalid field format. Use comma-separated: x,y,p,t")?;

    // Setup progress bar
    let progress = if args.quiet {
        ProgressBar::hidden()
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap(),
        );
        pb.set_message("Decoding...");
        pb
    };

    let start_time = Instant::now();

    // Open the input and output before decoding so event batches can be
    // written immediately instead of retaining the full recording in memory.
    progress.set_message(format!(
        "Decoding {:?}...",
        args.input.file_name().unwrap_or_default()
    ));

    let mut reader = EventFileReader::open(&args.input, DEFAULT_BATCH_BYTES)
        .context("Failed to open EVT3 file")?;
    let metadata = reader.metadata().clone();
    let mut output = StreamingOutput::open(&args.output, &metadata, field_order)?;
    let mut trigger_writer: Option<TriggerCsvWriter<File>> = None;
    let mut batch = ColumnarEventSink::default();
    let mut event_count = 0_u64;
    let mut trigger_count = 0_u64;

    while reader
        .read_next_into(&mut batch)
        .context("Failed to decode EVT3 file")?
    {
        output.write(&batch.cd).context("Failed to write output")?;
        event_count += batch.cd.len() as u64;

        if !batch.triggers.is_empty() {
            if trigger_writer.is_none() {
                if let Some(path) = &args.triggers {
                    trigger_writer = Some(TriggerCsvWriter::new(
                        File::create(path).context("Failed to create trigger output")?,
                    ));
                }
            }
            if let Some(writer) = &mut trigger_writer {
                writer.write_columns(&batch.triggers)?;
            }
            trigger_count += batch.triggers.len() as u64;
        }

        batch.clear();
    }

    output.finish(event_count)?;
    if let Some(writer) = &mut trigger_writer {
        writer.flush()?;
    }

    let total_duration = start_time.elapsed();

    progress.finish_with_message(format!(
        "Done! Decoded {} events in {:.2}s (sensor: {}x{})",
        event_count,
        total_duration.as_secs_f64(),
        metadata.width,
        metadata.height
    ));

    if !args.quiet {
        // Print summary
        let events_per_sec = event_count as f64 / total_duration.as_secs_f64();
        eprintln!();
        eprintln!("Summary:");
        eprintln!("  Input:        {:?}", args.input);
        eprintln!("  Output:       {:?}", args.output);
        eprintln!("  CD Events:    {}", event_count);
        eprintln!("  Triggers:     {}", trigger_count);
        eprintln!("  Sensor:       {}x{}", metadata.width, metadata.height);
        eprintln!("  Duration:     {:.3}s", total_duration.as_secs_f64());
        eprintln!("  Throughput:   {:.0} events/s", events_per_sec);
    }

    Ok(())
}
