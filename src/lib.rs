//! # REAPER Regions Library
//!
//! This library parses [REAPER DAW](https://www.reaper.fm/) region markers from WAV files.
//! It extracts markers, regions, and their associated metadata from
//! WAV files rendered from REAPER with markers or markers + regions included.
//! These are stored in RIFF `'cue '`, `'labl'`, and `'smpl'` chunks by REAPER.
//! In order for this to work properly, two conditions must be met:
//!
//! 1. The project **must** have at least one marker or region defined in the track view:
//! <img alt="Track showing a marker and two regions" src="images/track.png" width="511">
//!
//! 2. The WAV file **must** be rendered with Regions or Regions + Markers, and there must be at least one marker or region in the time range of the rendered output.
//! <img alt="Render with markers or markers + regions" src="images/render.png" width="610">
//!    - The "Write BWF ('bext') chunk" checkbox is **optional** and has no effect on the regions/markers:
//!
//! This library **might** work with WAV files exported from other DAWs with markers/regions,
//! but many of them do not support embedding markers or loop regions in exported WAV files.
//! If you find another DAW whose exports this library can read, please let me know.
//!
//! ## Features
//! - Parses REAPER region markers and cues from WAV files
//! - Extracts region names, start/end sample offsets, and start/end times and durations (in seconds)
//! - Supports both markers (single points) and regions (start/end ranges)
//! - Provides human-readable and machine-readable output formats
//!
//! ## Supported WAV Chunks
//! - `cue ` - Cue points with unique IDs and positions
//! - `labl` - Labels associated with cue points
//! - `smpl` - Sampler data including loop points
//! - `LIST` - List chunks containing additional metadata
//!
//! ## Example
//! ```rust,no_run
//! use reaper_regions::parse_markers_from_file;
//!
//! let data = parse_markers_from_file("path/to/audio.wav").unwrap();
//! println!("{data:#?}");
//! ```
//!
//! **Output:**
//! ```rust,ignore
//! WavData {
//!     path: "tests/fixtures/3-markers-3-regions-overlapping_stripped.wav",
//!     sample_rate: 48000,
//!     markers: [
//!         Marker {
//!             id: 1,
//!             name: "Region 1",
//!             type: Region,
//!             start: 290708,
//!             end: Some(
//!                 886374,
//!             ),
//!             start_time: 6.056416666666666,
//!             end_time: Some(
//!                 18.466125,
//!             ),
//!             duration: Some(
//!                 12.409708333333334,
//!             ),
//!         },
//!         Marker {
//!             id: 2,
//!             name: "Marker 1",
//!             type: Marker,
//!             start: 383050,
//!             end: None,
//!             start_time: 7.980208333333334,
//!             end_time: None,
//!             duration: None,
//!         },
//!         Marker {
//!             id: 3,
//!             name: "Region 2",
//!             type: Region,
//!             start: 1060229,
//!             end: Some(
//!                 1496290,
//!             ),
//!             start_time: 22.088104166666668,
//!             end_time: Some(
//!                 31.172708333333333,
//!             ),
//!             duration: Some(
//!                 9.084604166666665,
//!             ),
//!         },
//!         ...
//!     ],
//!     reason: None,
//!     reason_text: None,
//! }
//! ```
//!
//! ## Installation
//! ```bash
//! cargo add reaper-regions --no-default-features
//! ```
//!
//! ## Motivation
//! I was motivated to create this tool because I needed to sync song regions from my
//! master mixdown created in REAPER with my video projects in [DaVinci Resolve](https://www.blackmagicdesign.com/products/davinciresolve)
//! for live concert video and audio productions.
//! Unfortunately, Resolve does not read markers or regions embedded in WAV files.
//! Also, the metadata exported by REAPER, as inspected with `ffprobe`, reports
//! incorrect end times for regions (possibly due to metadata spec limitations?),
//! necessitating this tool.
//!
//! ## Acknowledgements / License
//!
//! REAPER is a trademark and the copyright property of [Cockos, Incorporated](https://www.cockos.com/).
//! This library is free, open source, and MIT-licensed.
//! DaVinci Resolve is a trademark and the copyright property of [Blackmagic Design Pty. Ltd.](https://www.blackmagicdesign.com/)

pub mod wavtag;

use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error};
use strum::EnumMessage;
use wavtag::{ChunkType, RiffFile};

/// Reason for missing or incomplete markers in a WAV file.
///
/// These enum variants explain why marker parsing might yield incomplete results,
/// helping users understand the limitations of the parsed data.
#[derive(Debug, strum::EnumMessage, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reason {
    /// No label chunks were found in the file
    NoLabels,
    /// No 'smpl' (sampler) chunk was found in the file
    NoSamplerData,
    /// Labels and/or sampler data found but no 'cue ' chunk
    NoCuePoints,
    /// Metadata exists but couldn't be matched into markers
    NoMarkersMatched,
}

/// Error type for parsing operations.
///
/// This enum covers all possible errors that can occur during WAV file parsing,
/// including I/O errors, malformed chunks, and missing required data.
#[derive(Debug, wherror::Error)]
#[error(debug)]
pub enum ParseError {
    /// I/O error when reading the file
    Io(#[from] std::io::Error),
    /// File doesn't contain a WAVE tag
    #[error("no WAVE tag found")]
    NoWaveTag,
    /// File doesn't contain a RIFF tag
    #[error("no RIFF tag found")]
    NoRiffTag,
    /// Format chunk is missing
    MissingFormatChunk,
    /// Format chunk has invalid length
    #[error("Format chunk length: expected >= 8, got {0}")]
    InvalidFormatChunk(usize),
    /// Failed to convert bytes to little-endian integer
    #[error("bytes to little endian at step: {0}")]
    BytesToLe(String),
    /// Other parsing errors
    Other(String),
}

/// Result type for parsing operations.
pub type ParseResult = Result<WavData, ParseError>;

/// The complete result of parsing a WAV file for markers.
///
/// Contains all parsed markers along with file metadata and any parsing warnings.
#[derive(Debug, Default, Serialize)]
pub struct WavData {
    /// Path to the source WAV file
    pub path: String,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Vector of parsed markers and regions
    pub markers: Vec<Marker>,
    /// Reason for incomplete parsing, if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
    /// Human-readable description of the parsing reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_text: Option<String>,
}

impl WavData {
    /// Sets a reason for incomplete parsing.
    ///
    /// # Arguments
    /// * `reason` - The [`Reason`] variant describing why parsing was incomplete
    ///
    /// This also sets `reason_text` to the human-readable documentation from the enum.
    pub fn set_reason(&mut self, reason: Reason) {
        self.reason = Some(reason);
        self.reason_text = reason.get_documentation().map(ToString::to_string);
    }

    /// Clears any previously set parsing reason.
    ///
    /// Used when markers are successfully parsed or when resetting the state.
    pub fn clear_reason(&mut self) {
        self.reason = None;
        self.reason_text = None;
    }
}

/// Type of marker in the WAV file.
///
/// Distinguishes between simple markers (single points) and regions (ranges).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkerType {
    /// A simple marker representing a single point in time
    Marker,
    /// A region with both start and end points defining a range
    Region,
}

/// Represents a labeled marker or region in a Reaper WAV file.
///
/// Contains all metadata for a single marker or region including timing information
/// in both samples and seconds, and derived duration for regions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    /// Unique identifier matching the cue point in the WAV file
    pub id: u32,
    /// Name of the marker (from 'labl' chunk)
    pub name: String,
    /// Type of marker (Marker or Region)
    pub r#type: MarkerType,
    /// Start position in samples
    pub start: u32,
    /// End position in samples (None for simple markers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u32>,
    /// DERIVED: Start time in seconds
    #[serde(serialize_with = "serialize_f64")]
    pub start_time: f64,
    /// DERIVED: End time in seconds (None for simple markers)
    #[serde(
        serialize_with = "serialize_opt_f64",
        skip_serializing_if = "Option::is_none"
    )]
    pub end_time: Option<f64>,
    /// DERIVED: Duration in seconds (None for simple markers)
    #[serde(
        serialize_with = "serialize_opt_f64",
        skip_serializing_if = "Option::is_none"
    )]
    pub duration: Option<f64>,
}

/// Rounds a floating-point value to 3 decimal places.
///
/// # Arguments
/// * `value` - The floating-point value to round
///
/// # Returns
/// * `f64` - The rounded value
///
/// # Example
/// ```
/// use reaper_regions::round3;
/// let value = 1.234567;
/// assert_eq!(round3(value), 1.235);
/// ```
pub fn round3(value: f64) -> f64 {
    (value * 1_000.0).round() / 1_000.0
}

/// Custom serializer for f64 values.
///
/// Automatically rounds values to 3 decimal places during serialization.
fn serialize_f64<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_f64(round3(*value))
}

/// Custom serializer for optional f64 values.
///
/// Automatically rounds values to 3 decimal places during serialization.
fn serialize_opt_f64<S>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(value) => serializer.serialize_some(&round3(*value)),
        None => serializer.serialize_none(),
    }
}

impl Marker {
    /// Creates a new marker or region.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for the marker
    /// * `name` - Name/label of the marker
    /// * `start` - Start position in samples
    /// * `end` - End position in samples (None for markers, Some for regions)
    /// * `sample_rate` - Sample rate of the audio file in Hz
    ///
    /// # Returns
    /// * [`Marker`] - A new Marker instance with derived timing information
    ///
    /// # Example
    /// ```
    /// use reaper_regions::{Marker, MarkerType};
    ///
    /// // Create a marker
    /// let marker = Marker::new(1, "Verse".to_string(), 44100, None, 44100);
    /// assert_eq!(marker.r#type, MarkerType::Marker);
    /// assert_eq!(marker.start_time, 1.0);
    ///
    /// // Create a region
    /// let region = Marker::new(2, "Chorus".to_string(), 44100, Some(88200), 44100);
    /// assert_eq!(region.r#type, MarkerType::Region);
    /// assert_eq!(region.duration, Some(1.0));
    /// ```
    pub fn new(id: u32, name: String, start: u32, end: Option<u32>, sample_rate: u32) -> Self {
        let marker_type = if end.is_some() {
            MarkerType::Region
        } else {
            MarkerType::Marker
        };

        // Calculate derived time values
        let start_time = start as f64 / sample_rate as f64;
        let (end_time, duration) = match end {
            Some(end) => {
                let end_s = end as f64 / sample_rate as f64;
                let dur_s = end_s - start_time;
                (Some(end_s), Some(dur_s))
            }
            None => (None, None),
        };

        Marker {
            id,
            name,
            r#type: marker_type,
            start,
            end,
            start_time,
            end_time,
            duration,
        }
    }

    /// Formats the marker as a human-readable string.
    ///
    /// # Returns
    /// * `String` - Formatted description of the marker
    ///
    /// # Example
    /// ```
    /// use reaper_regions::Marker;
    ///
    /// let marker = Marker::new(1, "Intro".to_string(), 0, None, 44100);
    /// println!("{}", marker.format());
    /// // Output: "Marker (ID: 1): 'Intro'\n  Position: 0.000s (0 samples)"
    /// ```
    pub fn format(&self) -> String {
        match self.r#type {
            MarkerType::Region => {
                let end = self.end.unwrap();
                format!(
                    "Region (ID: {}): '{}'\n  Start: {:.3}s ({} samples), End: {:.3}s ({} samples), Duration: {:.3}s",
                    self.id,
                    self.name,
                    self.start_time,
                    self.start,
                    self.end_time.unwrap(),
                    end,
                    self.duration.unwrap()
                )
            }
            MarkerType::Marker => {
                format!(
                    "Marker (ID: {}): '{}'\n  Position: {:.3}s ({} samples)",
                    self.id, self.name, self.start_time, self.start
                )
            }
        }
    }
}

/// Parses all markers from a Reaper WAV file.
///
/// # Arguments
/// * `file_path` - Path to the WAV file to parse
///
/// # Returns
/// * [`ParseResult`] - Result containing parsed markers or an error
///
/// # Errors
/// * [`ParseError::Io`] - If the file cannot be read
/// * [`ParseError::NoRiffTag`] - If the file is not a valid RIFF file
/// * [`ParseError::NoWaveTag`] - If the file is not a valid WAV file
/// * [`ParseError::MissingFormatChunk`] - If the format chunk is missing
/// * [`ParseError::InvalidFormatChunk`] - If the format chunk is malformed
///
/// # Example
/// ```
/// use reaper_regions::parse_markers_from_file;
///
/// match parse_markers_from_file("audio.wav") {
///     Ok(markers) => {
///         println!("Found {} markers", markers.markers.len());
///     }
///     Err(e) => {
///         eprintln!("Failed to parse markers: {}", e);
///     }
/// }
/// ```
pub fn parse_markers_from_file(file_path: &str) -> Result<WavData, ParseError> {
    let file = std::fs::File::open(file_path)?;
    let riff_file = RiffFile::read(file, file_path.to_string()).map_err(|err| {
        let string = err.to_string();
        if string.contains("no RIFF tag found") {
            return ParseError::NoRiffTag;
        }
        if string.contains("no WAVE tag found") {
            return ParseError::NoWaveTag;
        }
        err.into()
    })?;

    // Get sample rate from format chunk
    let sample_rate = get_sample_rate(&riff_file)?;
    debug!("Sample rate: {} Hz", sample_rate);

    let mut result = WavData {
        path: file_path.to_string(),
        sample_rate,
        ..WavData::default()
    };

    // Parse labels
    let labels = parse_labels(&riff_file);
    debug!("Found {} label(s)", labels.len());

    // Parse sampler loops
    let sampler_data = parse_sampler_data(&riff_file)?;
    if sampler_data.is_none() {
        debug!("No sample loops found.");
        result.set_reason(Reason::NoSamplerData);
    }

    // Parse cue points for start positions
    let Some(cue_points) = parse_cue_points(&riff_file)? else {
        debug!("No cue points found.");
        result.set_reason(Reason::NoCuePoints);
        return Ok(result);
    };

    // Match everything together
    result.markers = match_markers(labels, sampler_data, cue_points, sample_rate);

    Ok(result)
}

/// Internal struct for label data.
#[derive(Debug, Clone)]
struct Label {
    cue_id: u32,
    name: String,
}

/// Parses the sample rate from the format chunk.
///
/// # Arguments
/// * `riff_file` - Reference to the parsed RIFF file
///
/// # Returns
/// * `Result<u32, ParseError>` - Sample rate in Hz or an error
///
/// # Errors
/// * [`ParseError::MissingFormatChunk`] - If format chunk is not found
/// * [`ParseError::InvalidFormatChunk`] - If format chunk is too short (< 8 bytes)
/// * [`ParseError::BytesToLe`] - If bytes cannot be converted to little-endian
fn get_sample_rate(riff_file: &RiffFile) -> Result<u32, ParseError> {
    let format_chunk = riff_file
        .find_chunk_by_type(ChunkType::Format)
        .ok_or(ParseError::MissingFormatChunk)?;
    // Format chunk structure for PCM:
    // Offset 0-1: Audio format (1 for PCM)
    // Offset 2-3: Number of channels
    // Offset 4-7: Sample rate (u32, little-endian)
    let len = format_chunk.data.len();
    if len < 8 {
        warn!("Format chunk too short: expected >= 8, got: {len}");
        return Err(ParseError::InvalidFormatChunk(len));
    }
    let sample_rate_bytes = &format_chunk.data[4..8];
    let sample_rate = u32::from_le_bytes(
        sample_rate_bytes
            .try_into()
            .map_err(|_| ParseError::BytesToLe("sample rate".into()))?,
    );
    Ok(sample_rate)
}

/// Parses all labels from the file (standalone or LIST chunks).
///
/// # Arguments
/// * `riff_file` - Reference to the parsed RIFF file
///
/// # Returns
/// * `Vec<Label>` - Vector of parsed labels
///
/// # Note
/// This function first looks for standalone 'labl' chunks, then falls back
/// to parsing labels from LIST-adtl chunks if no standalone labels are found.
fn parse_labels(riff_file: &RiffFile) -> Vec<Label> {
    let mut labels = Vec::new();
    let mut found_standalone_labels = false;

    // Look for standalone 'labl' chunks first
    debug!("=== LOOKING FOR STANDALONE LABEL CHUNKS ===");
    for chunk in &riff_file.chunks {
        if chunk.header == ChunkType::Label {
            found_standalone_labels = true;
            if chunk.data.len() >= 4 {
                // Convert first 4 bytes to u32 (cue_id)
                let cue_id_bytes: [u8; 4] = match chunk.data[0..4].try_into() {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        warn!("Failed to convert label chunk data to array of 4 bytes, skipping");
                        continue;
                    }
                };
                let cue_id = u32::from_le_bytes(cue_id_bytes);
                let name_bytes = &chunk.data[4..];
                let name = String::from_utf8_lossy(name_bytes)
                    .trim_end_matches('\0')
                    .to_string();

                // Use the name for logging before moving it into the Label
                debug!(
                    "  Found standalone Label -> Cue ID: {}, Name: '{}'",
                    cue_id, name
                );

                // Now create the Label with the name
                labels.push(Label { cue_id, name });
            }
        }
    }

    // If no standalone labels, parse the LIST-adtl chunk
    if !found_standalone_labels {
        debug!("=== PARSING LIST CHUNK ===");
        if let Some(list_chunk) = riff_file.find_chunk_by_type(ChunkType::List) {
            debug!("  LIST chunk size: {} bytes", list_chunk.data.len());

            if let Ok(list_labels) = parse_list_chunk_for_labels(list_chunk) {
                debug!("  Found {} label(s) in LIST chunk", list_labels.len());
                labels.extend(list_labels);
            }
        }
    }

    labels
}

/// Parses sampler chunk data to extract sample loops.
///
/// # Arguments
/// * `riff_file` - Reference to the parsed RIFF file
///
/// # Returns
/// * `Result<Option<Vec<wavtag::SampleLoop>>, ParseError>` - Sample loops or None if not found
///
/// # Errors
/// * [`ParseError::BytesToLe`] - If sampler chunk data cannot be parsed
fn parse_sampler_data(riff_file: &RiffFile) -> Result<Option<Vec<wavtag::SampleLoop>>, ParseError> {
    if let Some(smpl_chunk) = riff_file.find_chunk_by_type(ChunkType::Sampler) {
        let sampler_data = wavtag::SamplerChunk::from_chunk(smpl_chunk)?;
        debug!("Found {} sample loop(s)", sampler_data.sample_loops.len());
        Ok(Some(sampler_data.sample_loops))
    } else {
        warn!("No 'smpl' chunk found!");
        Ok(None)
    }
}

/// Parses 'labl' subchunks from a LIST-adtl chunk.
///
/// # Arguments
/// * `list_chunk` - Reference to the LIST chunk to parse
///
/// # Returns
/// * `Result<Vec<Label>, Box<dyn Error>>` - Vector of labels or an error
///
/// # Note
/// LIST-adtl chunks can contain multiple label subchunks. This function
/// iterates through the LIST chunk data to extract all 'labl' subchunks.
fn parse_list_chunk_for_labels(
    list_chunk: &wavtag::RiffChunk,
) -> Result<Vec<Label>, Box<dyn Error>> {
    let mut labels = Vec::new();
    let data = &list_chunk.data;

    if data.len() < 4 || &data[0..4] != b"adtl" {
        return Ok(labels);
    }

    let mut pos = 4;
    while pos + 8 <= data.len() {
        let sub_id = std::str::from_utf8(&data[pos..pos + 4]).unwrap_or("<invalid>");
        let sub_size = u32::from_le_bytes(
            data[pos + 4..pos + 8]
                .try_into()
                .map_err(|_| ParseError::BytesToLe("'labl' chunk".into()))?,
        ) as usize;

        if pos + 8 + sub_size > data.len() {
            break;
        }

        if sub_id == "labl" && sub_size >= 4 {
            let cue_id = u32::from_le_bytes(
                data[pos + 8..pos + 12]
                    .try_into()
                    .map_err(|_| ParseError::BytesToLe("cue ID".into()))?,
            );
            let text_start = pos + 12;
            let text_end = text_start + (sub_size - 4);
            let raw_text = &data[text_start..text_end];

            let name = String::from_utf8_lossy(raw_text)
                .trim_end_matches('\0')
                .to_string();

            debug!("    Found label: Cue ID={}, Name='{}'", cue_id, name);
            labels.push(Label { cue_id, name });
        }

        let padded_size = (sub_size + 1) & !1;
        pos += 8 + padded_size;
    }

    Ok(labels)
}

/// Matches labels with sampler loops to create complete markers/regions.
///
/// # Arguments
/// * `labels` - Vector of parsed labels with cue IDs and names
/// * `sampler_loops` - Vector of sampler loops containing end positions
/// * `cue_points` - HashMap of cue IDs to start positions (from 'cue ' chunk)
/// * `sample_rate` - Sample rate of the audio file
///
/// # Returns
/// * `Vec<Marker>` - Vector of complete markers/regions
///
/// # Algorithm
/// 1. Creates a label map from cue ID to name
/// 2. Creates a sampler map from cue ID to end position
/// 3. For each label, looks up its start position and end position (if any)
/// 4. Creates markers (no end) or regions (with end)
/// 5. Sorts markers by start time
fn match_markers(
    labels: Vec<Label>,
    sampler_loops: Option<Vec<wavtag::SampleLoop>>,
    cue_points: HashMap<u32, u32>, // NEW: Start positions from 'cue ' chunk
    sample_rate: u32,
) -> Vec<Marker> {
    let label_map: HashMap<u32, String> = labels
        .into_iter()
        .map(|label| (label.cue_id, label.name))
        .collect();

    let sampler_map: HashMap<u32, u32> = sampler_loops
        .unwrap_or_default()
        .into_iter()
        .map(|sl| (sl.id, sl.end))
        .collect();

    let mut markers = Vec::new();

    for (cue_id, name) in label_map {
        let end = sampler_map.get(&cue_id).copied();
        let start = cue_points.get(&cue_id).copied().unwrap_or(0); // Use real start or 0 if missing

        markers.push(Marker::new(cue_id, name, start, end, sample_rate));
    }

    // Sort markers by their start time for cleaner output
    markers.sort_by_key(|m| m.start);

    markers
}

/// Parses 'cue ' chunk to get cue point positions (start samples).
///
/// # Arguments
/// * `riff_file` - Reference to the parsed RIFF file
///
/// # Returns
/// * `Result<Option<HashMap<u32, u32>>, ParseError>` - Map of cue IDs to start positions, or None if not found
///
/// # Errors
/// * [`ParseError::BytesToLe`] - If cue chunk data cannot be parsed
///
/// # Note
/// Each cue point record is 24 bytes with the following structure:
/// - dwIdentifier (4 bytes): Cue ID
/// - dwPosition (4 bytes): Position
/// - fccChunk (4 bytes): Chunk type
/// - dwChunkStart (4 bytes): Chunk start
/// - dwBlockStart (4 bytes): Block start
/// - dwSampleOffset (4 bytes): Sample offset (used as start position)
fn parse_cue_points(riff_file: &RiffFile) -> Result<Option<HashMap<u32, u32>>, ParseError> {
    let mut cue_map = HashMap::new();

    let Some(cue_chunk) = riff_file.find_chunk_by_type(ChunkType::Cue) else {
        debug!("No 'cue ' chunk found");
        return Ok(None);
    };

    let data = &cue_chunk.data;
    if data.len() < 4 {
        warn!("expected 'cue ' chunk length >= 4, got {}", data.len());
        return Ok(None);
    }

    let num_cues = u32::from_le_bytes(
        data[0..4]
            .try_into()
            .map_err(|_| ParseError::BytesToLe("number of cues".into()))?,
    );
    debug!("Found {} cue points in 'cue ' chunk", num_cues);

    // Each cue point record is 24 bytes
    // Structure: dwIdentifier(4), dwPosition(4), fccChunk(4), dwChunkStart(4), dwBlockStart(4), dwSampleOffset(4)
    let record_size = 24;
    for i in 0..num_cues {
        let start = 4 + (i as usize * record_size);
        if start + record_size <= data.len() {
            let cue_id = u32::from_le_bytes(
                data[start..start + 4]
                    .try_into()
                    .map_err(|_| ParseError::BytesToLe("cue id".into()))?,
            );
            // The sample position is in dwSampleOffset at offset 20 within the record
            let sample_offset = u32::from_le_bytes(
                data[start + 20..start + 24]
                    .try_into()
                    .map_err(|_| ParseError::BytesToLe("sample offset".into()))?,
            );
            cue_map.insert(cue_id, sample_offset);
            debug!("  Cue ID {} -> Start sample: {}", cue_id, sample_offset);
        }
    }

    Ok(Some(cue_map))
}
