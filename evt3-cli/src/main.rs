//! EVT 3.0 decoder CLI application.
//!
//! Decodes Prophesee EVT 3.0 raw files to various output formats.

use anyhow::{Context, Result};
use clap::Parser;
use evt3_core::{output, Evt3Decoder, FieldOrder};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
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

    // Decode the file
    progress.set_message(format!(
        "Decoding {:?}...",
        args.input.file_name().unwrap_or_default()
    ));

    let mut decoder = Evt3Decoder::new();
    let result = decoder
        .decode_file(&args.input)
        .context("Failed to decode EVT3 file")?;

    let decode_duration = start_time.elapsed();

    if !args.quiet {
        progress.set_message(format!(
            "Decoded {} CD events, {} trigger events in {:.2}s",
            result.cd_events.len(),
            result.trigger_events.len(),
            decode_duration.as_secs_f64()
        ));
    }

    // Determine output format from extension
    let output_ext = args
        .output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("csv");

    progress.set_message(format!(
        "Writing to {:?}...",
        args.output.file_name().unwrap_or_default()
    ));

    match output_ext.to_lowercase().as_str() {
        "csv" => {
            output::write_csv(
                &args.output,
                &result.cd_events,
                Some(&result.metadata),
                field_order,
            )
            .context("Failed to write CSV output")?;
        }
        "bin" => {
            output::write_binary(&args.output, &result.cd_events, &result.metadata)
                .context("Failed to write binary output")?;
        }
        _ => {
            anyhow::bail!(
                "Unsupported output format: .{}. Use .csv or .bin",
                output_ext
            );
        }
    }

    // Write trigger events if requested
    if let Some(trigger_path) = &args.triggers {
        if !result.trigger_events.is_empty() {
            output::write_trigger_csv(trigger_path, &result.trigger_events)
                .context("Failed to write trigger CSV")?;

            if !args.quiet {
                progress.set_message(format!(
                    "Wrote {} trigger events to {:?}",
                    result.trigger_events.len(),
                    trigger_path.file_name().unwrap_or_default()
                ));
            }
        }
    }

    let total_duration = start_time.elapsed();

    progress.finish_with_message(format!(
        "Done! Decoded {} events in {:.2}s (sensor: {}x{})",
        result.cd_events.len(),
        total_duration.as_secs_f64(),
        result.metadata.width,
        result.metadata.height
    ));

    if !args.quiet {
        // Print summary
        let events_per_sec = result.cd_events.len() as f64 / total_duration.as_secs_f64();
        eprintln!();
        eprintln!("Summary:");
        eprintln!("  Input:        {:?}", args.input);
        eprintln!("  Output:       {:?}", args.output);
        eprintln!("  CD Events:    {}", result.cd_events.len());
        eprintln!("  Triggers:     {}", result.trigger_events.len());
        eprintln!(
            "  Sensor:       {}x{}",
            result.metadata.width, result.metadata.height
        );
        eprintln!("  Duration:     {:.3}s", total_duration.as_secs_f64());
        eprintln!("  Throughput:   {:.0} events/s", events_per_sec);
    }

    Ok(())
}
