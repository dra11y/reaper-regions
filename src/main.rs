//! # REAPER Regions CLI
//!
//! Command-line interface for extracting Reaper region markers from WAV files.
//! This tool parses WAV files created with Reaper DAW and outputs markers
//! and regions in various formats.
//!
//! ## Usage
//! ```bash
//! reaper-regions audio.wav
//! reaper-regions audio.wav --format json
//! reaper-regions audio.wav --format csv --no-header
//! reaper-regions audio.wav --debug
//! ```
//!
//! ## Output Formats
//! - Human-readable (default): Easy to read in terminal
//! - JSON: JavaScript Object Notation
//! - Delimited, with or without headers, for piping into other programs.
//!   - CSV: Comma-separated
//!   - TSV: Tab-separated
//!   - PSV: Pipe-separated
//!
//! ## Installation
//! ```bash
//! cargo install reaper-regions
//! ```
//!
//! ## Example JSON output
//!
//! ```json
//! {
//!   "markers": [
//!     {
//!       "duration": 12.41,
//!       "end": 886374,
//!       "end_time": 18.466,
//!       "id": 1,
//!       "name": "Region 1",
//!       "start": 290708,
//!       "start_time": 6.056,
//!       "type": "Region"
//!     },
//!     {
//!       "id": 2,
//!       "name": "Marker 1",
//!       "start": 383050,
//!       "start_time": 7.98,
//!       "type": "Marker"
//!     },
//!     {
//!       "duration": 9.085,
//!       "end": 1496290,
//!       "end_time": 31.173,
//!       "id": 3,
//!       "name": "Region 2",
//!       "start": 1060229,
//!       "start_time": 22.088,
//!       "type": "Region"
//!     }, ...
//!   ],
//!   "path": "tests/fixtures/3-markers-3-regions-overlapping_stripped.wav",
//!   "sample_rate": 48000
//! }
//! ```
//!
//! ## Example PSV output
//! ```
//! type|id|name|start|end|start_time|end_time|duration|sample_rate
//! region|1|Region 1|290708|886374|6.056|18.466|12.410|48000
//! marker|2|Marker 1|383050||7.980|||48000
//! region|3|Region 2|1060229|1496290|22.088|31.173|9.085|48000
//! ```
//!
//! ## Example human output
//! ```
//! File: tests/fixtures/3-markers-3-regions-overlapping_stripped.wav
//! Sample rate: 48000 Hz
//! Total markers: 6
//!
//! Region (ID: 1): 'Region 1'
//!   Start: 6.056s (290708 samples)
//!   End: 18.466s (886374 samples)
//!   Duration: 12.410s (12.409708333333334 samples)
//!
//! Marker (ID: 2): 'Marker 1'
//!   Position: 7.980s (383050 samples)
//!
//! Region (ID: 3): 'Region 2'
//!   Start: 22.088s (1060229 samples)
//!   End: 31.173s (1496290 samples)
//!   Duration: 9.085s (9.084604166666665 samples)
//! ...
//! ```
//!
//! ## Acknowledgements / License
//!
//! REAPER is a trademark and the copyright property of [Cockos, Incorporated](https://www.cockos.com/).
//! This library is free, open source, and MIT-licensed.

use clap::{Parser, ValueEnum};
use env_logger::Builder;
use log::{debug, error};
use reaper_regions::{ParseResult, parse_markers_from_file, round3};
use serde_json;
use std::io;
use strum::EnumMessage;

/// Extract Reaper region markers from WAV files.
#[derive(Parser)]
#[command(version, about, arg_required_else_help = true)]
struct Cli {
    /// Path to the input WAV file containing Reaper markers.
    ///
    /// The file must be a valid WAV file with RIFF structure and
    /// may contain Reaper-specific chunks for markers and regions.
    file: String,

    /// Output format for displaying parsed markers.
    ///
    /// Choose from human-readable, JSON, or various delimited formats.
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    /// Enable debug logging for troubleshooting parsing issues.
    ///
    /// When enabled, shows detailed information about chunk parsing,
    /// label matching, and any warnings encountered.
    #[arg(short, long)]
    debug: bool,

    /// Omit header row in CSV/TSV/PSV output formats.
    ///
    /// Useful when piping output to other tools that don't expect headers.
    #[arg(short, long)]
    no_header: bool,
}

/// Supported output formats for marker data.
///
/// Each format is optimized for different use cases:
/// - Human: Terminal viewing and manual inspection
/// - JSON: Programmatic consumption and data exchange
/// - CSV/TSV/PSV: Spreadsheets and data processing pipelines
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum OutputFormat {
    /// JSON format (machine-readable)
    ///
    /// Outputs complete marker data as a JSON object with
    /// file metadata, sample rate, and array of markers.
    Json,
    /// Comma-separated values (CSV)
    ///
    /// Tabular format compatible with spreadsheets and databases.
    /// Fields are comma-separated and quoted as needed.
    Csv,
    /// Tab-separated values (TSV)
    ///
    /// Tabular format with tabs as separators, useful for
    /// Unix tools that expect tab-separated data.
    Tsv,
    /// Pipe-separated values (PSV)
    ///
    /// Tabular format with pipes as separators, often used
    /// in Unix pipelines where commas or tabs might appear in data.
    Psv,
    /// Human-readable format
    ///
    /// Formatted for easy reading in terminal output with
    /// clear labels, indentation, and grouping.
    Human,
}

/// Main entry point for the Reaper Regions CLI.
///
/// This function:
/// 1. Parses command-line arguments
/// 2. Configures logging based on debug flag
/// 3. Calls the parser to extract markers from the WAV file
/// 4. Routes output to the appropriate format handler
///
/// # Exit Codes
/// - 0: Success
/// - 1: Parsing error or invalid file
///
/// # Panics
/// May panic if logging cannot be initialized or if output
/// formatting fails (though errors are typically handled gracefully).
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
    let result = parse_markers_from_file(&cli.file);

    // Output in requested format
    match cli.format {
        OutputFormat::Json => output_json(&result),
        OutputFormat::Csv => output_delimited(&result, ',', !cli.no_header),
        OutputFormat::Tsv => output_delimited(&result, '\t', !cli.no_header),
        OutputFormat::Psv => output_delimited(&result, '|', !cli.no_header),
        OutputFormat::Human => output_human(&result),
    }
}

/// Outputs parsed markers in JSON format.
///
/// # Arguments
/// * `result` - The parsing result containing markers or an error
///
/// # Output
/// Prints JSON to stdout with the following structure:
/// ```json
/// {
///   "path": "audio.wav",
///   "sample_rate": 44100,
///   "markers": [...],
///   "reason": "NoLabels",
///   "reason_text": "No label chunks were found in the file"
/// }
/// ```
///
/// If parsing fails, outputs an error object:
/// ```json
/// {
///   "error": "Error message here"
/// }
/// ```
fn output_json(result: &ParseResult) {
    let value = match result {
        Ok(result) => serde_json::to_value(result).unwrap(),
        Err(error) => serde_json::json!({
            "error": error.to_string()
        }),
    };
    let output = serde_json::to_string_pretty(&value).unwrap();
    println!("{output}");
}

/// Outputs parsed markers in delimited format (CSV, TSV, PSV).
///
/// # Arguments
/// * `result` - The parsing result containing markers
/// * `delimiter` - Character to use as field delimiter
/// * `include_header` - Whether to include a header row
///
/// # Fields
/// The output includes these columns:
/// - type: "marker" or "region"
/// - id: Unique marker ID
/// - name: Marker label
/// - start: Start position in samples
/// - end: End position in samples (empty for markers)
/// - start_time: Start time in seconds (rounded to 3 decimals)
/// - end_time: End time in seconds (empty for markers)
/// - duration: Duration in seconds (empty for markers)
/// - sample_rate: File sample rate in Hz
///
/// # Panics
/// Will panic if CSV writing fails, though this is rare for stdout.
fn output_delimited(result: &ParseResult, delimiter: char, include_header: bool) {
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            error!("{error}");
            std::process::exit(1);
        }
    };

    let mut wtr = csv::WriterBuilder::new()
        .delimiter(delimiter as u8)
        .from_writer(io::stdout());

    // Header
    if include_header {
        let _ = wtr.write_record(&[
            "type",
            "id",
            "name",
            "start",
            "end",
            "start_time",
            "end_time",
            "duration",
            "sample_rate",
        ]);
    }

    // Data rows
    for marker in &result.markers {
        let _ = wtr.write_record(&[
            format!("{:?}", marker.r#type).to_lowercase(),
            marker.id.to_string(),
            marker.name.clone(),
            marker.start.to_string(),
            marker.end.map(|v| v.to_string()).unwrap_or_default(),
            // Use the pre-calculated fields.
            format!("{:.3}", round3(marker.start_time)),
            marker
                .end_time
                .map(|v| format!("{:.3}", round3(v)))
                .unwrap_or_default(),
            marker
                .duration
                .map(|v| format!("{:.3}", round3(v)))
                .unwrap_or_default(),
            result.sample_rate.to_string(),
        ]);
    }

    let _ = wtr.flush();
}

/// Outputs parsed markers in human-readable format.
///
/// # Arguments
/// * `result` - The parsing result containing markers
///
/// # Output
/// Prints formatted output with:
/// - File path
/// - Sample rate
/// - Marker count
/// - Parsing reason (if any)
/// - Detailed list of markers and regions with timing information
///
/// Example output:
/// ```
/// File: audio.wav
/// Sample rate: 44100 Hz
/// Total markers: 3
///
/// Region (ID: 1): 'Verse'
///   Start: 0.000s (0 samples)
///   End: 4.410s (44100 samples)
///   Duration: 4.410s
///
/// Marker (ID: 2): 'Chorus Start'
///   Position: 4.410s (44100 samples)
/// ```
fn output_human(result: &ParseResult) {
    let data = match result {
        Ok(data) => data,
        Err(error) => {
            error!("{error}");
            std::process::exit(1);
        }
    };

    debug!("{data:#?}");

    println!("File: {}", data.path);

    println!("Sample rate: {} Hz", data.sample_rate);

    println!("Total markers: {}", data.markers.len());

    if let Some(reason) = data.reason {
        let reason = match reason.get_documentation() {
            Some(docs) => format!("{reason:?}: {docs}"),
            None => format!("{reason:?}"),
        };
        println!("Reason: {reason}")
    }

    println!();

    for marker in data.markers.iter() {
        match marker.end {
            Some(end_sample) => {
                // This is a region
                println!("Region (ID: {}): '{}'", marker.id, marker.name);
                println!(
                    "  Start: {:.3}s ({} samples)",
                    marker.start_time, marker.start
                );
                println!(
                    "  End: {:.3}s ({} samples)",
                    marker.end_time.unwrap(),
                    end_sample
                );
                println!(
                    "  Duration: {:.3}s ({} samples)",
                    marker.duration.unwrap(),
                    marker.duration.unwrap()
                );
            }
            None => {
                // This is a simple marker
                println!("Marker (ID: {}): '{}'", marker.id, marker.name);
                println!(
                    "  Position: {:.3}s ({} samples)",
                    marker.start_time, marker.start
                );
            }
        }

        println!();
    }
}
