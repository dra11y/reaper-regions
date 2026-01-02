use std::collections::HashMap;
use std::error::Error;
use wavtag::{ChunkType, RiffFile};

#[derive(Debug, Clone)]
struct Label {
    cue_id: u32,
    name: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let file = std::fs::File::open("/Users/tom/Desktop/test.wav")?;
    let riff_file = RiffFile::read(file, "test.wav".to_string())?;

    // -- PART 1: COLLECT ALL REGION NAMES --
    let mut labels = Vec::new();

    // Strategy 1: Look for standalone 'labl' chunks first
    for chunk in &riff_file.chunks {
        if chunk.header == ChunkType::Label {
            // 'labl' chunk structure: [cue_id (4 bytes)][null-terminated text]
            if chunk.data.len() >= 4 {
                let cue_id = u32::from_le_bytes(chunk.data[0..4].try_into()?);
                let name_bytes = &chunk.data[4..];
                let name = String::from_utf8_lossy(name_bytes)
                    .trim_end_matches('\0')
                    .to_string();

                let label = Label { cue_id, name };
                labels.push(label.clone());
                println!(
                    "Found Label chunk -> Cue ID: {}, Name: '{}'",
                    label.cue_id, label.name
                );
            }
        }
    }

    // Strategy 2: If no standalone labels, parse the LIST-adtl chunk
    if labels.is_empty() {
        if let Some(list_chunk) = riff_file.find_chunk_by_type(ChunkType::List) {
            println!("Parsing LIST chunk for 'labl' subchunks...");
            let list_labels = parse_list_chunk_for_labels(list_chunk)?;
            labels.extend(list_labels);
        }
    }

    // Create a HashMap for quick lookup by cue_id
    let label_map: HashMap<u32, String> = labels
        .into_iter()
        .map(|label| (label.cue_id, label.name))
        .collect();

    // -- PART 2: GET REGION START/END TIMES FROM SMPL CHUNK --
    let sampler_data = if let Some(smpl_chunk) = riff_file.find_chunk_by_type(ChunkType::Sampler) {
        wavtag::SamplerChunk::from_chunk(smpl_chunk)?
    } else {
        println!("No 'smpl' chunk found!");
        return Ok(());
    };

    // -- PART 3: PRINT MATCHED REGIONS --
    println!("\n=== FINAL REGION LIST ===");
    for (i, sample_loop) in sampler_data.sample_loops.iter().enumerate() {
        let name = label_map
            .get(&sample_loop.id)
            .map(|s| s.as_str())
            .unwrap_or("<No Name>");

        // Convert sample positions to seconds (assuming 48kHz sample rate)
        let start_sec = sample_loop.start as f64 / 48000.0;
        let end_sec = sample_loop.end as f64 / 48000.0;

        println!("Region {} (ID: {}): '{}'", i + 1, sample_loop.id, name);
        println!(
            "  Start: {:.3}s ({} samples), End: {:.3}s ({} samples)",
            start_sec, sample_loop.start, end_sec, sample_loop.end
        );
    }

    Ok(())
}

// Helper function to parse 'labl' subchunks from a LIST-adtl chunk
fn parse_list_chunk_for_labels(
    list_chunk: &wavtag::RiffChunk,
) -> Result<Vec<Label>, Box<dyn Error>> {
    let mut labels = Vec::new();
    let data = &list_chunk.data;

    // LIST chunk must start with "adtl" for associated data
    if data.len() < 4 || &data[0..4] != b"adtl" {
        return Ok(labels);
    }

    let mut pos = 4; // Start after "adtl"
    while pos + 8 <= data.len() {
        // Need at least subchunk ID (4) + size (4)
        let sub_id = std::str::from_utf8(&data[pos..pos + 4]).unwrap_or("");
        let sub_size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into()?) as usize;

        if sub_id == "labl" && pos + 12 + sub_size <= data.len() {
            let cue_id = u32::from_le_bytes(data[pos + 8..pos + 12].try_into()?);
            let text_end = data[pos + 12..pos + 12 + sub_size]
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(sub_size - 4);
            let name = String::from_utf8_lossy(&data[pos + 12..pos + 12 + text_end])
                .trim_end_matches('\0')
                .to_string();

            labels.push(Label { cue_id, name });
        }
        // Move to next subchunk (data is padded to even byte boundary)
        pos += 8 + ((sub_size + 1) & !1);
    }

    Ok(labels)
}
