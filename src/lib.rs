mod wavtag;

use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error};
use strum::EnumMessage;
use wavtag::{ChunkType, RiffFile};

/// Reason for missing or incomplete markers
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

/// Parsing error
#[derive(Debug, wherror::Error)]
#[error(debug)]
pub enum ParseError {
    Io(#[from] std::io::Error),
    #[error("no WAVE tag found")]
    NoWaveTag,
    #[error("no RIFF tag found")]
    NoRiffTag,
    MissingFormatChunk,
    #[error("Format chunk length: expected >= 8, got {0}")]
    InvalidFormatChunk(usize),
    #[error("bytes to little endian at step: {0}")]
    BytesToLe(String),
    Other(String),
}

pub type ParseResult = Result<Markers, ParseError>;

/// The result of parsing a WAV file
#[derive(Debug, Default, Serialize)]
pub struct Markers {
    pub path: String,
    pub sample_rate: u32,
    pub markers: Vec<Marker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_text: Option<String>,
}

impl Markers {
    pub fn set_reason(&mut self, reason: Reason) {
        self.reason = Some(reason);
        self.reason_text = reason.get_documentation().map(ToString::to_string);
    }

    pub fn clear_reason(&mut self) {
        self.reason = None;
        self.reason_text = None;
    }
}

/// Type of the marker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkerType {
    /// A simple marker (single point)
    Marker,
    /// A region with start and end
    Region,
}

/// Represents a labeled marker or region in a Reaper WAV file
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    /// Unique identifier matching the cue point
    pub id: u32,
    /// Name (from 'labl' chunk)
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

pub fn round3(value: f64) -> f64 {
    (value * 1_000.0).round() / 1_000.0
}

// Custom serializer for f64
fn serialize_f64<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_f64(round3(*value))
}

// Custom serializer for Option<f64>
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
    /// Create a new marker or region
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

    /// Format marker as a string
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

/// Parse all markers from a Reaper WAV file
pub fn parse_markers_from_file(file_path: &str) -> Result<Markers, ParseError> {
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

    let mut result = Markers {
        path: file_path.to_string(),
        sample_rate,
        ..Markers::default()
    };

    // Parse labels
    let labels = parse_labels(&riff_file);
    debug!("Found {} label(s)", labels.len());

    // Parse sampler loops
    let Some(sampler_data) = parse_sampler_data(&riff_file)? else {
        debug!("No sample loops found.");
        result.set_reason(Reason::NoSamplerData);
        return Ok(result);
    };

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

/// Internal struct for label data
#[derive(Debug, Clone)]
struct Label {
    cue_id: u32,
    name: String,
}

/// Parse sample rate from format chunk
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

/// Parse all labels from the file (standalone or LIST chunks)
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

/// Parse sampler chunk data
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

/// Parse 'labl' subchunks from a LIST-adtl chunk
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

/// Match labels with sampler loops to create regions
fn match_markers(
    labels: Vec<Label>,
    sampler_loops: Vec<wavtag::SampleLoop>,
    cue_points: HashMap<u32, u32>, // NEW: Start positions from 'cue ' chunk
    sample_rate: u32,
) -> Vec<Marker> {
    let label_map: HashMap<u32, String> = labels
        .into_iter()
        .map(|label| (label.cue_id, label.name))
        .collect();

    let sampler_map: HashMap<u32, u32> = sampler_loops
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

/// Parse 'cue ' chunk to get cue point positions (start samples)
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
