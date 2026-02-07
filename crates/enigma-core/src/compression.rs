use crate::error::Result;

/// Compress a chunk using zstd at the given level (1-22, default 3).
pub fn compress_chunk(data: &[u8], level: i32) -> Result<Vec<u8>> {
    zstd::encode_all(data, level).map_err(|e| crate::error::EnigmaError::Compression(e.to_string()))
}

/// Decompress a zstd-compressed chunk.
pub fn decompress_chunk(data: &[u8]) -> Result<Vec<u8>> {
    zstd::decode_all(data).map_err(|e| crate::error::EnigmaError::Compression(e.to_string()))
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
