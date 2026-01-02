use env_logger::{Builder, Env};
use log::info;
use reaper_region_reader::parse_regions_from_file;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger to show info level by default
    // RUST_LOG can override this (e.g., RUST_LOG=debug cargo run -- file.wav)
    Builder::from_env(Env::default().default_filter_or("info")).init();

    // Get command line arguments
    let args: Vec<String> = env::args().collect();

    // Check if a filename was provided
    if args.len() < 2 {
        eprintln!("Usage: {} <wav_file>", args[0]);
        eprintln!("Example: {} /path/to/your/audio.wav", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];

    // Parse regions
    let regions = parse_regions_from_file(file_path)?;

    // Display results - using info! so they appear with default logging
    info!("=== REGIONS FOUND ===");
    info!("File: {}", file_path);
    info!("Total regions: {}\n", regions.len());

    for region in regions {
        info!("{}", region.format());
        info!(""); // Empty line between regions
    }

    Ok(())
}
