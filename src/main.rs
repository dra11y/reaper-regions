use log::{LevelFilter, debug};
use std::collections::HashMap;
use std::error::Error;
use wavtag::{ChunkType, RiffFile};

#[derive(Debug, Clone)]
struct Label {
    cue_id: u32,
    name: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let file = std::fs::File::open("/Users/tom/Desktop/test.wav")?;
    let riff_file = RiffFile::read(file, "test.wav".to_string())?;

    // DEBUG: List all chunk types found
    debug!("=== CHUNK DISCOVERY ===");
    for (i, chunk) in riff_file.chunks.iter().enumerate() {
        debug!("  Chunk {}: {:?}", i, chunk.header);
    }

    // -- PART 1: COLLECT ALL REGION NAMES --
    let mut labels = Vec::new();
    let mut found_standalone_labels = false;

    // Strategy 1: Look for standalone 'labl' chunks first
    debug!("\n=== LOOKING FOR STANDALONE LABEL CHUNKS ===");
    for chunk in &riff_file.chunks {
        if chunk.header == ChunkType::Label {
            found_standalone_labels = true;
            if chunk.data.len() >= 4 {
                let cue_id = u32::from_le_bytes(chunk.data[0..4].try_into()?);
                let name_bytes = &chunk.data[4..];
                let name = String::from_utf8_lossy(name_bytes)
                    .trim_end_matches('\0')
                    .to_string();

                let label = Label { cue_id, name };
                labels.push(label.clone());
                debug!(
                    "  Found standalone Label -> Cue ID: {}, Name: '{}'",
                    label.cue_id, label.name
                );
            }
        }
    }

    // Strategy 2: If no standalone labels, parse the LIST-adtl chunk
    if !found_standalone_labels {
        debug!("\n=== PARSING LIST CHUNK ===");
        if let Some(list_chunk) = riff_file.find_chunk_by_type(ChunkType::List) {
            debug!("  LIST chunk size: {} bytes", list_chunk.data.len());

            // Print raw hex of first 32 bytes for inspection
            let preview_len = std::cmp::min(32, list_chunk.data.len());
            debug!("  First {} bytes (hex):", preview_len);
            if log::max_level() == LevelFilter::Debug {
                for chunk in list_chunk.data[0..preview_len].chunks(8) {
                    print!("    ");
                    for &byte in chunk {
                        print!("{:02x} ", byte);
                    }
                }
            }

            let list_labels = parse_list_chunk_for_labels(list_chunk)?;
            debug!("  Found {} label(s) in LIST chunk:", list_labels.len());
            for label in &list_labels {
                debug!("    Cue ID: {}, Name: '{}'", label.cue_id, label.name);
            }
            labels.extend(list_labels);
        } else {
            debug!("  No LIST chunk found either.");
        }
    }

    debug!("\n=== TOTAL LABELS COLLECTED ===");
    debug!("  Count: {}", labels.len());
    for label in &labels {
        debug!("    ID: {} -> '{}'", label.cue_id, label.name);
    }

    // Create a HashMap for quick lookup by cue_id
    let label_map: HashMap<u32, String> = labels
        .into_iter()
        .map(|label| (label.cue_id, label.name))
        .collect();

    // -- PART 2: GET REGION START/END TIMES FROM SMPL CHUNK --
    debug!("\n=== PARSING SMPL CHUNK ===");
    let sampler_data = if let Some(smpl_chunk) = riff_file.find_chunk_by_type(ChunkType::Sampler) {
        let data = wavtag::SamplerChunk::from_chunk(smpl_chunk)?;
        debug!("  Found {} sample loop(s):", data.sample_loops.len());
        for (i, loop_data) in data.sample_loops.iter().enumerate() {
            debug!(
                "    Loop {}: ID={}, Start={}, End={}",
                i + 1,
                loop_data.id,
                loop_data.start,
                loop_data.end
            );
        }
        data
    } else {
        debug!("  No 'smpl' chunk found!");
        return Ok(());
    };

    // -- PART 3: PRINT MATCHED REGIONS --
    debug!("\n=== FINAL REGION MATCHING ===");
    debug!(
        "  Label map keys: {:?}",
        label_map.keys().collect::<Vec<_>>()
    );
    debug!(
        "  Sampler loop IDs: {:?}",
        sampler_data
            .sample_loops
            .iter()
            .map(|l| l.id)
            .collect::<Vec<_>>()
    );

    debug!("\n=== FINAL REGION LIST ===");
    for (i, sample_loop) in sampler_data.sample_loops.iter().enumerate() {
        let name = label_map
            .get(&sample_loop.id)
            .map(|s| s.as_str())
            .unwrap_or("<No Name>");

        // Convert sample positions to seconds (assuming 48kHz sample rate)
        let start_sec = sample_loop.start as f64 / 48000.0;
        let end_sec = sample_loop.end as f64 / 48000.0;

        debug!("Region {} (ID: {}): '{}'", i + 1, sample_loop.id, name);
        debug!(
            "  Start: {:.3}s ({} samples), End: {:.3}s ({} samples)",
            start_sec, sample_loop.start, end_sec, sample_loop.end
        );
    }

    Ok(())
}

// parse 'labl' subchunks from a LIST-adtl chunk
fn parse_list_chunk_for_labels(
    list_chunk: &wavtag::RiffChunk,
) -> Result<Vec<Label>, Box<dyn std::error::Error>> {
    let mut labels = Vec::new();
    let data = &list_chunk.data;

    if data.len() < 4 || &data[0..4] != b"adtl" {
        return Ok(labels);
    }

    let mut pos = 4;
    while pos + 8 <= data.len() {
        let sub_id = std::str::from_utf8(&data[pos..pos + 4]).unwrap_or("<invalid>");
        let sub_size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into()?) as usize;

        // FIXED: The sub_size INCLUDES the 4-byte cue_id field.
        // Check: 8 (ID+size) + sub_size must fit.
        if pos + 8 + sub_size > data.len() {
            break; // Malformed or end of data
        }

        if sub_id == "labl" && sub_size >= 4 {
            let cue_id = u32::from_le_bytes(data[pos + 8..pos + 12].try_into()?);
            // Text length is sub_size - 4 (for cue_id). Find null terminator.
            let text_start = pos + 12;
            let text_end = text_start + (sub_size - 4);
            let raw_text = &data[text_start..text_end];

            let name = String::from_utf8_lossy(raw_text)
                .trim_end_matches('\0')
                .to_string();

            labels.push(Label { cue_id, name });
        }
        // Move to next subchunk. Pad sub_size to an even number of bytes.
        let padded_size = (sub_size + 1) & !1;
        pos += 8 + padded_size;
    }

    Ok(labels)
}
