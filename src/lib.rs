use log::{debug, error, warn};
use std::collections::HashMap;
use std::error::Error;
use wavtag::{ChunkType, RiffFile};

/// Represents a labeled region in a Reaper WAV file
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// Unique identifier matching the cue point
    pub id: u32,
    /// Name of the region (from 'labl' chunk)
    pub name: String,
    /// Start position in samples
    pub start_sample: u32,
    /// End position in samples
    pub end_sample: u32,
    /// Sample rate of the audio file
    pub sample_rate: u32,
}

impl Region {
    /// Create a new region
    pub fn new(
        id: u32,
        name: String,
        start_sample: u32,
        end_sample: u32,
        sample_rate: u32,
    ) -> Self {
        Region {
            id,
            name,
            start_sample,
            end_sample,
            sample_rate,
        }
    }

    /// Get start time in seconds
    pub fn start_seconds(&self) -> f64 {
        self.start_sample as f64 / self.sample_rate as f64
    }

    /// Get end time in seconds
    pub fn end_seconds(&self) -> f64 {
        self.end_sample as f64 / self.sample_rate as f64
    }

    /// Get duration in seconds
    pub fn duration_seconds(&self) -> f64 {
        self.end_seconds() - self.start_seconds()
    }

    /// Get duration in samples
    pub fn duration_samples(&self) -> u32 {
        self.end_sample - self.start_sample
    }

    /// Format region as a string
    pub fn format(&self) -> String {
        format!(
            "Region {} (ID: {}): '{}'\n  Start: {:.3}s ({} samples), End: {:.3}s ({} samples), Duration: {:.3}s",
            self.id,
            self.id,
            self.name,
            self.start_seconds(),
            self.start_sample,
            self.end_seconds(),
            self.end_sample,
            self.duration_seconds()
        )
    }
}

/// Parse all regions from a Reaper WAV file
pub fn parse_regions_from_file(file_path: &str) -> Result<Vec<Region>, Box<dyn Error>> {
    let file = std::fs::File::open(file_path)?;
    let riff_file = RiffFile::read(file, file_path.to_string())?;

    debug!("=== CHUNK DISCOVERY ===");
    for (i, chunk) in riff_file.chunks.iter().enumerate() {
        debug!("  Chunk {}: {:?}", i, chunk.header);
    }

    // Get sample rate from format chunk
    let sample_rate = get_sample_rate(&riff_file)?;
    debug!("Sample rate: {} Hz", sample_rate);

    // Parse labels
    let labels = parse_labels(&riff_file);
    debug!("Found {} label(s)", labels.len());

    // Parse sampler chunks
    let sampler_data = parse_sampler_data(&riff_file)?;

    // Match labels with sampler loops
    let regions = match_regions(labels, sampler_data, sample_rate);

    Ok(regions)
}

/// Internal struct for label data
#[derive(Debug, Clone)]
struct Label {
    cue_id: u32,
    name: String,
}

/// Parse sample rate from format chunk
fn get_sample_rate(riff_file: &RiffFile) -> Result<u32, Box<dyn Error>> {
    if let Some(format_chunk) = riff_file.find_chunk_by_type(ChunkType::Format) {
        // Format chunk structure for PCM:
        // Offset 0-1: Audio format (1 for PCM)
        // Offset 2-3: Number of channels
        // Offset 4-7: Sample rate (u32, little-endian)
        if format_chunk.data.len() >= 8 {
            let sample_rate_bytes = &format_chunk.data[4..8];
            let sample_rate = u32::from_le_bytes(sample_rate_bytes.try_into()?);
            return Ok(sample_rate);
        }
    }

    warn!("Could not determine sample rate from format chunk, defaulting to 48000 Hz");
    Ok(48000) // Default to 48kHz if we can't determine
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
fn parse_sampler_data(riff_file: &RiffFile) -> Result<Vec<wavtag::SampleLoop>, Box<dyn Error>> {
    if let Some(smpl_chunk) = riff_file.find_chunk_by_type(ChunkType::Sampler) {
        let sampler_data = wavtag::SamplerChunk::from_chunk(smpl_chunk)?;
        debug!("Found {} sample loop(s)", sampler_data.sample_loops.len());
        Ok(sampler_data.sample_loops)
    } else {
        error!("No 'smpl' chunk found!");
        Err("No sampler chunk found in WAV file".into())
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
        let sub_size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into()?) as usize;

        if pos + 8 + sub_size > data.len() {
            break;
        }

        if sub_id == "labl" && sub_size >= 4 {
            let cue_id = u32::from_le_bytes(data[pos + 8..pos + 12].try_into()?);
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
fn match_regions(
    labels: Vec<Label>,
    sampler_loops: Vec<wavtag::SampleLoop>,
    sample_rate: u32,
) -> Vec<Region> {
    let label_map: HashMap<u32, String> = labels
        .into_iter()
        .map(|label| (label.cue_id, label.name))
        .collect();

    debug!("Label map keys: {:?}", label_map.keys().collect::<Vec<_>>());
    debug!(
        "Sampler loop IDs: {:?}",
        sampler_loops.iter().map(|l| l.id).collect::<Vec<_>>()
    );

    let mut regions = Vec::new();

    for sample_loop in sampler_loops {
        let name = label_map
            .get(&sample_loop.id)
            .map(|s| s.as_str())
            .unwrap_or("<No Name>")
            .to_string();

        regions.push(Region::new(
            sample_loop.id,
            name,
            sample_loop.start,
            sample_loop.end,
            sample_rate,
        ));
    }

    regions
}
