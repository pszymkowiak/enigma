use std::io::Read;

use crate::error::Result;

/// Maximum decompressed output size (64 MB) to prevent decompression bombs.
pub const MAX_DECOMPRESS_SIZE: usize = 64 * 1024 * 1024;

/// Compress a chunk using zstd at the given level (1-22, default 3).
pub fn compress_chunk(data: &[u8], level: i32) -> Result<Vec<u8>> {
    zstd::encode_all(data, level).map_err(|e| crate::error::EnigmaError::Compression(e.to_string()))
}

/// Decompress a zstd-compressed chunk.
///
/// Enforces a maximum output size of [`MAX_DECOMPRESS_SIZE`] to prevent
/// decompression bombs.
pub fn decompress_chunk(data: &[u8]) -> Result<Vec<u8>> {
    let decoder = zstd::Decoder::new(data)
        .map_err(|e| crate::error::EnigmaError::Compression(e.to_string()))?;

    // Read up to MAX_DECOMPRESS_SIZE + 1 to detect overflow
    let mut limited = decoder.take(MAX_DECOMPRESS_SIZE as u64 + 1);
    let mut output = Vec::new();
    limited
        .read_to_end(&mut output)
        .map_err(|e| crate::error::EnigmaError::Compression(e.to_string()))?;

    if output.len() > MAX_DECOMPRESS_SIZE {
        return Err(crate::error::EnigmaError::Compression(format!(
            "Decompressed data exceeds maximum size of {} bytes",
            MAX_DECOMPRESS_SIZE
        )));
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let original = b"hello world, this is a test of zstd compression in enigma";
        let compressed = compress_chunk(original, 3).unwrap();
        let decompressed = decompress_chunk(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn empty_data() {
        let compressed = compress_chunk(b"", 3).unwrap();
        let decompressed = decompress_chunk(&compressed).unwrap();
        assert!(decompressed.is_empty());
    }
}
