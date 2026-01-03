use clap::{Parser, ValueEnum};
use env_logger::Builder;
use log::error;
use reaper_regions::ParseResult;
use reaper_regions::parse_markers_from_file;
use serde_json;
use std::io;
use std::process;
use strum::EnumMessage;

/// Extract Reaper region markers from WAV files
#[derive(Parser)]
#[command(name = "reaper-region-reader")]
#[command(version = "0.1.0")]
#[command(about = "Extracts Reaper region markers from WAV files", long_about = None)]
struct Cli {
    /// Input WAV file
    file: String,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Omit header row in CSV/TSV/PSV output
    #[arg(long)]
    no_header: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum OutputFormat {
    /// JSON format (machine-readable)
    Json,
    /// Comma-separated values
    Csv,
    /// Tab-separated values
    Tsv,
    /// Pipe-separated values
    Psv,
    /// Human-readable format
    Human,
}

fn main() {
    let cli = Cli::parse();

    // Configure logging
    let log_level = if cli.debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    };

    Builder::new()
        .filter_level(log_level)
        .format_target(false)
        .format_timestamp(None)
        .init();

    // Parse regions
    let result = match parse_markers_from_file(&cli.file) {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to parse '{}': {}", cli.file, e);
            process::exit(1);
        }
    };

    // Output in requested format
    match cli.format {
        OutputFormat::Json => output_json(&result),
        OutputFormat::Csv => output_delimited(&result, ',', !cli.no_header),
        OutputFormat::Tsv => output_delimited(&result, '\t', !cli.no_header),
        OutputFormat::Psv => output_delimited(&result, '|', !cli.no_header),
        OutputFormat::Human => output_human(&result),
    }
}

/// JSON output (machine-readable)
fn output_json(result: &ParseResult) {
    let output = serde_json::json!({
        "file": result.path,
        "sample_rate": result.markers.first().map(|r| r.sample_rate),
        "markers": result.markers,
        "reason": result.reason,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

/// Delimited output (CSV, TSV, PSV)
fn output_delimited(result: &ParseResult, delimiter: char, include_header: bool) {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(delimiter as u8)
        .from_writer(io::stdout());

    // Header
    if include_header {
        let _ = wtr.write_record(&[
            "type",
            "id",
            "name",
            "start_sample",
            "end_sample",
            "start_seconds",
            "end_seconds",
            "duration_seconds",
            "sample_rate",
        ]);
    }

    // Data rows
    for marker in &result.markers {
        let _ = wtr.write_record(&[
            format!("{:?}", marker.marker_type).to_lowercase(),
            marker.id.to_string(),
            marker.name.clone(),
            marker.start_sample.to_string(),
            marker.end_sample.map(|v| v.to_string()).unwrap_or_default(),
            // Use the pre-calculated fields. Format to 6 decimal places.
            format!("{:.6}", marker.start_sec),
            marker
                .end_sec
                .map(|v| format!("{:.6}", v))
                .unwrap_or_default(),
            marker
                .duration_sec
                .map(|v| format!("{:.6}", v))
                .unwrap_or_default(),
            marker.sample_rate.to_string(),
        ]);
    }

    let _ = wtr.flush();
}

/// Human-readable output
fn output_human(result: &ParseResult) {
    println!("=== MARKERS FOUND ===");
    println!("File: {}", result.path);

    if let Some(first_region) = result.markers.first() {
        println!("Sample rate: {} Hz", first_region.sample_rate);
    }

    println!("Total markers: {}", result.markers.len());

    if let Some(reason) = result.reason {
        match reason.get_documentation() {
            Some(docs) => println!("Reason: {reason:?}: {docs}\n"),
            None => println!("Reason: {reason:?}\n"),
        }
    }

    for (i, marker) in result.markers.iter().enumerate() {
        match marker.end_sample {
            Some(end_sample) => {
                // This is a region
                println!("Region (ID: {}): '{}'", marker.id, marker.name);
                println!(
                    "  Start: {:.3}s ({} samples)",
                    marker.start_seconds(),
                    marker.start_sample
                );
                println!(
                    "  End: {:.3}s ({} samples)",
                    marker.end_seconds().unwrap(),
                    end_sample
                );
                println!(
                    "  Duration: {:.3}s ({} samples)",
                    marker.duration_seconds().unwrap(),
                    marker.duration_samples().unwrap()
                );
            }
            None => {
                // This is a simple marker
                println!("Marker (ID: {}): '{}'", marker.id, marker.name);
                println!(
                    "  Position: {:.3}s ({} samples)",
                    marker.start_seconds(),
                    marker.start_sample
                );
            }
        }

        if i < result.markers.len() - 1 {
            println!();
        }
    }
}
