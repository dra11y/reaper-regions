//! Golden tests for CLI output formats.
//!
//! These tests compare the CLI output against expected "golden" files.
//! Use `cargo test -- --test test_cli_goldens --bless` to update golden files.

use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use std::path::{Path, PathBuf};

// Define output formats and their file extensions
const FORMATS: &[(&str, &str)] = &[
    ("json", "json"),
    ("csv", "csv"),
    ("tsv", "tsv"),
    ("psv", "psv"),
    ("human", "human"),
];

/// Test helper to run CLI with given arguments
fn run_cli(wav_path: &Path, format: &str) -> String {
    let mut cmd = cargo_bin_cmd!();

    cmd.arg(wav_path);

    if format != "human" {
        cmd.arg("--format").arg(format);
    }

    let output = cmd
        .output()
        .expect(&format!("Failed to run CLI with format {}", format));

    // Check for successful execution
    assert!(output.status.success(), "CLI failed: {:?}", output);

    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Get the golden file path for a test case
fn golden_path(wav_path: &Path, format_ext: &str) -> PathBuf {
    let wav_name = wav_path.file_stem().unwrap().to_string_lossy();
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    test_dir
        .join("goldens")
        .join(format!("{}.{}", wav_name, format_ext))
}

#[test]
fn test_cli_goldens() {
    // Check if we're in bless mode
    let bless = std::env::var("BLESS").ok().is_some();

    let fixtures_dir = Path::new("tests/fixtures");

    // Find all .wav files in fixtures directory
    let wav_files: Vec<PathBuf> = fs::read_dir(&fixtures_dir)
        .expect("Failed to read fixtures directory")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()? == "wav" {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    assert!(!wav_files.is_empty(), "No .wav files found in fixtures");

    for wav_path in wav_files {
        let wav_name = wav_path.file_name().unwrap().to_string_lossy();

        for (format, ext) in FORMATS {
            println!("\nTesting {} with format {}...", wav_path.display(), format);

            let output = run_cli(&wav_path, format);
            let golden_file = golden_path(&wav_path, ext);

            if bless {
                // Create directory if it doesn't exist
                if let Some(parent) = golden_file.parent() {
                    fs::create_dir_all(parent).expect("Failed to create goldens directory");
                }

                // Write golden file
                fs::write(&golden_file, &output)
                    .expect(&format!("Failed to write golden file: {:?}", golden_file));

                println!("  ✓ Updated golden file: {:?}", golden_file);
            } else {
                // Read expected output from golden file
                if !golden_file.exists() {
                    panic!(
                        "Golden file not found: {:?}\nRun with --bless to generate it.\n\nOutput was:\n{}",
                        golden_file, output
                    );
                }

                let expected = fs::read_to_string(&golden_file)
                    .expect(&format!("Failed to read golden file: {:?}", golden_file));

                // Compare with simple diff (good enough for most cases)
                if output.trim() != expected.trim() {
                    eprintln!("❌ Mismatch for {} with format {}", wav_name, format);
                    eprintln!("--- Expected (golden)");
                    eprintln!("+++ Actual (CLI output)");

                    // Simple line-by-line diff
                    let expected_lines: Vec<&str> = expected.trim().lines().collect();
                    let actual_lines: Vec<&str> = output.trim().lines().collect();

                    for (i, (exp, act)) in
                        expected_lines.iter().zip(actual_lines.iter()).enumerate()
                    {
                        if exp != act {
                            eprintln!("Line {}:", i + 1);
                            eprintln!("- {}", exp);
                            eprintln!("+ {}", act);
                        }
                    }

                    // Handle different lengths
                    if expected_lines.len() != actual_lines.len() {
                        eprintln!("Different number of lines:");
                        eprintln!("- Expected: {} lines", expected_lines.len());
                        eprintln!("+ Actual: {} lines", actual_lines.len());
                    }

                    panic!("Output mismatch for {} with format {}", wav_name, format);
                }

                println!("  ✓ Output matches golden file");
            }
        }
    }

    if bless {
        println!("\n✅ All golden files updated successfully!");
    }
}

/// Test the --no-header flag for delimited formats
#[test]
fn test_cli_no_header() {
    let wav_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("3-markers-3-regions-overlapping_stripped.wav");

    // Test CSV without header
    let mut cmd = cargo_bin_cmd!();
    let output = cmd
        .arg(&wav_path)
        .arg("--format")
        .arg("csv")
        .arg("--no-header")
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let output_str = String::from_utf8_lossy(&output.stdout);

    // Check that first line is NOT the header
    let first_line = output_str.lines().next().unwrap();
    assert!(!first_line.contains("type") && !first_line.contains("id"));

    // Check that we have the expected number of lines (6 markers, no header)
    assert_eq!(output_str.lines().count(), 6);
}

/// Test debug flag
#[test]
fn test_cli_debug() {
    let wav_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("3-markers-3-regions-overlapping_stripped.wav");

    let mut cmd = cargo_bin_cmd!();
    let output = cmd
        .arg(&wav_path)
        .arg("--debug")
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let output_str = String::from_utf8_lossy(&output.stderr);

    // Debug output should include debug information
    assert!(output_str.contains("Found label"));
    assert!(output_str.contains("Found 6 label(s)"));
    assert!(output_str.contains("Found 3 sample loop(s)"));
    assert!(output_str.contains("Cue ID 6"));
}
