use clap::{Arg, ArgAction, Command};
use env_logger::Builder;
use log::{error, warn};
use reaper_region_reader::parse_regions_from_file;
use serde_json;
use std::io;
use std::process;

fn main() {
    // Set up command line arguments
    let matches = Command::new("reaper-region-reader")
        .version("0.1.0")
        .author("Your Name")
        .about("Extracts Reaper region markers from WAV files")
        .arg(
            Arg::new("FILE")
                .help("Input WAV file")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("format")
                .short('f')
                .long("format")
                .value_name("FORMAT")
                .help("Output format")
                .value_parser(["json", "csv", "tsv", "psv", "human"])
                .default_value("human"),
        )
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .help("Enable debug logging")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-header")
                .long("no-header")
                .help("Omit header row in CSV/TSV/PSV output")
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    // Configure logging
    let log_level = if matches.get_flag("debug") {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    };

    Builder::new()
        .filter_level(log_level)
        .format_target(false)
        .format_timestamp(None)
        .init();

    let file_path = matches.get_one::<String>("FILE").unwrap();
    let format = matches.get_one::<String>("format").unwrap();
    let no_header = matches.get_flag("no-header");

    // Parse regions
    let regions = match parse_regions_from_file(file_path) {
        Ok(regions) => regions,
        Err(e) => {
            error!("Failed to parse '{}': {}", file_path, e);
            process::exit(1);
        }
    };

    if regions.is_empty() {
        warn!("No regions found in '{}'", file_path);
        process::exit(0);
    }

    // Output in requested format
    match format.as_str() {
        "json" => output_json(&regions, file_path),
        "csv" => output_delimited(&regions, ',', !no_header),
        "tsv" => output_delimited(&regions, '\t', !no_header),
        "psv" => output_delimited(&regions, '|', !no_header),
        "human" => output_human(&regions, file_path),
        _ => unreachable!(), // Clap validates this
    }
}

/// JSON output (machine-readable)
fn output_json(regions: &[reaper_region_reader::Region], file_path: &str) {
    let output = serde_json::json!({
        "file": file_path,
        "sample_rate": regions.first().map(|r| r.sample_rate).unwrap_or(0),
        "region_count": regions.len(),
        "regions": regions
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

/// Delimited output (CSV, TSV, PSV)
fn output_delimited(
    regions: &[reaper_region_reader::Region],
    delimiter: char,
    include_header: bool,
) {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(delimiter as u8)
        .from_writer(io::stdout());

    // Header
    if include_header {
        let _ = wtr.write_record(&[
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
    for region in regions {
        let _ = wtr.write_record(&[
            region.id.to_string(),
            region.name.clone(),
            region.start_sample.to_string(),
            region.end_sample.to_string(),
            format!("{:.6}", region.start_seconds()),
            format!("{:.6}", region.end_seconds()),
            format!("{:.6}", region.duration_seconds()),
            region.sample_rate.to_string(),
        ]);
    }

    let _ = wtr.flush();
}

/// Human-readable output
fn output_human(regions: &[reaper_region_reader::Region], file_path: &str) {
    println!("=== REGIONS FOUND ===");
    println!("File: {}", file_path);

    if let Some(first_region) = regions.first() {
        println!("Sample rate: {} Hz", first_region.sample_rate);
    }

    println!("Total regions: {}\n", regions.len());

    for (i, region) in regions.iter().enumerate() {
        println!("Region {} (ID: {}): '{}'", i + 1, region.id, region.name);
        println!(
            "  Start: {:.3}s ({} samples)",
            region.start_seconds(),
            region.start_sample
        );
        println!(
            "  End: {:.3}s ({} samples)",
            region.end_seconds(),
            region.end_sample
        );
        println!(
            "  Duration: {:.3}s ({} samples)",
            region.duration_seconds(),
            region.duration_samples()
        );

        if i < regions.len() - 1 {
            println!();
        }
    }
}
