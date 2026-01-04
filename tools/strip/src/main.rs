use clap::Parser;
use reaper_regions::wavtag::{ChunkType, RiffFile};
use std::{error::Error, fs, path::Path};

/// Tool to strip audio data from Reaper WAV files while preserving markers and regions.
#[derive(Parser)]
#[command()]
struct Cli {
    /// Input folder containing WAV files to process
    input_folder: String,

    /// Output folder for stripped WAV files (default: "stripped" in input folder)
    #[arg(short, long)]
    output_folder: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .skip(1)
        .find(|p| p.join("Cargo.toml").exists())
        .unwrap()
        .join("tests/fixtures");

    // Create output folder if it doesn't exist
    fs::create_dir_all(&output_dir)?;

    println!("Processing WAV files in: {}", cli.input_folder);
    println!("Output folder: {}", output_dir.display());

    // Find all .wav files recursively
    let mut processed = 0;
    let mut errors = 0;

    for entry in walkdir::WalkDir::new(&cli.input_folder)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if let Some(ext) = path.extension() {
            if ext.eq_ignore_ascii_case("wav") {
                match process_file(path, &output_dir) {
                    Ok(_) => processed += 1,
                    Err(e) => {
                        eprintln!("Error processing {}: {}", path.display(), e);
                        errors += 1;
                    }
                }
            }
        }
    }

    println!("\nDone! Processed: {}, Errors: {}", processed, errors);
    Ok(())
}

fn process_file(input_path: &Path, output_dir: &Path) -> Result<(), Box<dyn Error>> {
    let file_stem = input_path
        .file_stem()
        .ok_or_else(|| format!("Invalid file name: {}", input_path.display()))?
        .to_string_lossy();

    let output_filename = format!("{}_stripped.wav", file_stem);
    let output_path = Path::new(output_dir).join(output_filename);

    strip_audio_data(
        input_path.to_string_lossy().as_ref(),
        output_path.to_string_lossy().as_ref(),
    )
}

/// Strips the audio data from a WAV file, leaving only the header, format, and metadata chunks.
fn strip_audio_data(input_path: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
    // Read and parse the input file
    let file = fs::File::open(input_path)?;
    let riff_file = RiffFile::read(file, input_path.to_string())?;

    // Prepare output buffer
    let mut out = Vec::new();

    // Write RIFF header
    out.extend(b"RIFF");
    out.extend(&0u32.to_le_bytes()); // Placeholder for total size
    out.extend(b"WAVE");

    // Copy all chunks except the data chunk (or replace it with a zero‑length one)
    for chunk in &riff_file.chunks {
        match chunk.header {
            ChunkType::Data => {
                // Write a zero‑length data chunk instead of the original audio data
                out.extend(b"data");
                out.extend(&0u32.to_le_bytes()); // size = 0
                // No audio data bytes are appended
            }
            _ => {
                // Copy the chunk header and data exactly as it appears
                out.extend(chunk.header.clone().to_tag());
                out.extend(&(chunk.data.len() as u32).to_le_bytes());
                out.extend(&chunk.data);
            }
        }
    }

    // Update the RIFF size field (total file size minus 8 bytes for "RIFF" and size)
    let riff_size = out.len() as u32 - 8;
    (&mut out[4..8]).copy_from_slice(&riff_size.to_le_bytes());

    // Write the result to disk
    fs::write(output_path, &out)?;

    let original_size = fs::metadata(input_path)?.len();
    let reduction = ((original_size - out.len() as u64) as f64 / original_size as f64) * 100.0;

    println!(
        "Stripped {} -> {} ({} KB, {:.1}% reduction)",
        Path::new(input_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        Path::new(output_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        out.len() / 1024,
        reduction
    );
    Ok(())
}
