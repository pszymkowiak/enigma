use crate::chunk::ChunkEngine;
use crate::error::{EnigmaError, Result};
use crate::types::{ChunkHash, RawChunk};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// Fixed-size chunking engine.
pub struct FixedSizeChunkEngine {
    chunk_size: usize,
}

impl FixedSizeChunkEngine {
    pub fn new(chunk_size: usize) -> Self {
        Self { chunk_size }
    }
}

impl Default for FixedSizeChunkEngine {
    fn default() -> Self {
        Self::new(4 * 1024 * 1024) // 4 MB
    }
}

impl ChunkEngine for FixedSizeChunkEngine {
    fn chunk_file(&self, path: &Path) -> Result<Vec<RawChunk>> {
        let mut file = std::fs::File::open(path).map_err(|e| {
            EnigmaError::Chunking(format!("Failed to open {}: {e}", path.display()))
        })?;

        let mut chunks = Vec::new();
        let mut offset = 0u64;
        let mut buf = vec![0u8; self.chunk_size];

        loop {
            let mut total_read = 0;
            // Read exactly chunk_size bytes (or until EOF)
            while total_read < self.chunk_size {
                match file.read(&mut buf[total_read..]) {
                    Ok(0) => break,
                    Ok(n) => total_read += n,
                    Err(e) => return Err(EnigmaError::Chunking(format!("Read error: {e}"))),
                }
            }

            if total_read == 0 {
                break;
            }

            let data = buf[..total_read].to_vec();
            let hash = {
                let mut hasher = Sha256::new();
                hasher.update(&data);
                ChunkHash(hasher.finalize().into())
            };

            chunks.push(RawChunk {
                data,
                hash,
                offset,
                length: total_read,
            });

            offset += total_read as u64;
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
    fn fixed_chunks_exact_multiple() {
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![0xABu8; 2048];
        file.write_all(&data).unwrap();

        let engine = FixedSizeChunkEngine::new(1024);
        let chunks = engine.chunk_file(file.path()).unwrap();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].length, 1024);
        assert_eq!(chunks[1].length, 1024);
        assert_eq!(chunks[0].offset, 0);
        assert_eq!(chunks[1].offset, 1024);
    }

    #[test]
    fn fixed_chunks_with_remainder() {
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![0xABu8; 1500];
        file.write_all(&data).unwrap();

        let engine = FixedSizeChunkEngine::new(1024);
        let chunks = engine.chunk_file(file.path()).unwrap();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].length, 1024);
        assert_eq!(chunks[1].length, 476);
    }

    #[test]
    fn fixed_empty_file() {
        let file = NamedTempFile::new().unwrap();
        let engine = FixedSizeChunkEngine::new(1024);
        let chunks = engine.chunk_file(file.path()).unwrap();
        assert!(chunks.is_empty());
    }
}
