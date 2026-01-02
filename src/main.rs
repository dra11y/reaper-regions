use std::error::Error;
use wavtag::{ChunkType, RiffFile};

fn main() -> Result<(), Box<dyn Error>> {
    let file = std::fs::File::open("/Users/tom/Desktop/test.wav")?;
    let riff_file = RiffFile::read(file, "test.wav".to_string())?;

    // 1. Find the sampler (smpl) chunk for region end times
    if let Some(smpl_chunk) = riff_file.find_chunk_by_type(ChunkType::Sampler) {
        let sampler_data = wavtag::SamplerChunk::from_chunk(smpl_chunk)?;
        for (i, sample_loop) in sampler_data.sample_loops.iter().enumerate() {
            println!(
                "Region {}: Start Sample: {}, End Sample: {}",
                i + 1,
                sample_loop.start,
                sample_loop.end
            );
        }
    }

    // 2. Find the cue chunk for region start points and IDs
    if let Some(cue_chunk) = riff_file.find_chunk_by_type(ChunkType::Cue) {
        // Parse the cue chunk data here (see code structure from wavtag source)
        println!("Cue chunk found. Size: {} bytes", cue_chunk.data.len());
    }

    // 3. Find LIST->adtl->labl chunks for region names
    if let Some(list_chunk) = riff_file.find_chunk_by_type(ChunkType::List) {
        // Parse LIST chunk for 'labl' subchunks here
        println!("List chunk found. Size: {} bytes", list_chunk.data.len());
    }

    Ok(())
}
