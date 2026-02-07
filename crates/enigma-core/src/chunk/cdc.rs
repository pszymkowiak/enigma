use crate::chunk::ChunkEngine;
use crate::error::{EnigmaError, Result};
use crate::types::{ChunkHash, RawChunk};
use fastcdc::v2020::FastCDC;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Content-Defined Chunking engine using FastCDC.
///
/// Default target size: 4 MB, with min = target/4, max = target*4.
pub struct CdcChunkEngine {
    min_size: u32,
    avg_size: u32,
    max_size: u32,
}

impl CdcChunkEngine {
    pub fn new(target_size: u32) -> Self {
        Self {
            min_size: target_size / 4,
            avg_size: target_size,
            max_size: target_size * 4,
        }
    }
}

impl Default for CdcChunkEngine {
    fn default() -> Self {
        Self::new(4 * 1024 * 1024) // 4 MB
    }
}

impl ChunkEngine for CdcChunkEngine {
    fn chunk_file(&self, path: &Path) -> Result<Vec<RawChunk>> {
        let data = std::fs::read(path).map_err(|e| {
            EnigmaError::Chunking(format!("Failed to read {}: {e}", path.display()))
        })?;

        if data.is_empty() {
            return Ok(vec![]);
        }

        let chunker = FastCDC::new(&data, self.min_size, self.avg_size, self.max_size);
        let mut chunks = Vec::new();

        for entry in chunker {
            let chunk_data = data[entry.offset..entry.offset + entry.length].to_vec();
            let hash = {
                let mut hasher = Sha256::new();
                hasher.update(&chunk_data);
                ChunkHash(hasher.finalize().into())
            };

            chunks.push(RawChunk {
                data: chunk_data,
                hash,
                offset: entry.offset as u64,
                length: entry.length,
            });
        }

        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn cdc_chunks_small_file() {
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![0xABu8; 1024]; // 1 KB — smaller than min chunk
        file.write_all(&data).unwrap();

        let engine = CdcChunkEngine::default();
        let chunks = engine.chunk_file(file.path()).unwrap();

        assert!(!chunks.is_empty());
        // Small file should be a single chunk
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].length, 1024);
    }

    #[test]
    fn cdc_chunks_large_file() {
        let mut file = NamedTempFile::new().unwrap();
        // 10 MB of pseudo-random data using a simple PRNG for variability
        let mut rng_state: u64 = 0xDEADBEEF;
        let data: Vec<u8> = (0..10 * 1024 * 1024)
            .map(|_| {
                rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
                (rng_state >> 33) as u8
            })
            .collect();
        file.write_all(&data).unwrap();

        let engine = CdcChunkEngine::default();
        let chunks = engine.chunk_file(file.path()).unwrap();

        assert!(chunks.len() > 1);

        // Verify all data is covered
        let total: usize = chunks.iter().map(|c| c.length).sum();
        assert_eq!(total, data.len());

        // Verify offsets are contiguous
        let mut expected_offset = 0u64;
        for chunk in &chunks {
            assert_eq!(chunk.offset, expected_offset);
            expected_offset += chunk.length as u64;
        }
    }

    #[test]
    fn cdc_empty_file() {
        let file = NamedTempFile::new().unwrap();
        let engine = CdcChunkEngine::default();
        let chunks = engine.chunk_file(file.path()).unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn cdc_deterministic_hashes() {
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![0x42u8; 2048];
        file.write_all(&data).unwrap();

        let engine = CdcChunkEngine::default();
        let chunks1 = engine.chunk_file(file.path()).unwrap();
        let chunks2 = engine.chunk_file(file.path()).unwrap();

        assert_eq!(chunks1.len(), chunks2.len());
        for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(c1.hash, c2.hash);
        }
    }
}
