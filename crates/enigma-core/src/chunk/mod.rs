mod cdc;
mod fixed;

pub use cdc::CdcChunkEngine;
pub use fixed::FixedSizeChunkEngine;

use crate::error::Result;
use crate::types::RawChunk;
use std::path::Path;

/// Trait for splitting files into content-addressable chunks.
pub trait ChunkEngine: Send + Sync {
    /// Split a file into raw chunks, computing the SHA-256 hash of each.
    fn chunk_file(&self, path: &Path) -> Result<Vec<RawChunk>>;
}
