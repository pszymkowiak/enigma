use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::types::ChunkHash;

/// Compute the SHA-256 hash of data.
pub fn compute_hash(data: &[u8]) -> ChunkHash {
    let mut hasher = Sha256::new();
    hasher.update(data);
    ChunkHash(hasher.finalize().into())
}

/// Constant-time comparison of two chunk hashes.
pub fn hashes_equal(a: &ChunkHash, b: &ChunkHash) -> bool {
    a.0.ct_eq(&b.0).into()
}

/// Check if a chunk hash already exists in a set (for dedup decisions).
/// Returns the index if found.
pub fn find_duplicate(hash: &ChunkHash, existing: &[ChunkHash]) -> Option<usize> {
    existing.iter().position(|h| hashes_equal(h, hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_deterministic() {
        let data = b"hello world";
        let h1 = compute_hash(data);
        let h2 = compute_hash(data);
        assert!(hashes_equal(&h1, &h2));
    }

    #[test]
    fn hash_different_data() {
        let h1 = compute_hash(b"hello");
        let h2 = compute_hash(b"world");
        assert!(!hashes_equal(&h1, &h2));
    }

    #[test]
    fn find_duplicate_found() {
        let hashes: Vec<ChunkHash> = (0..5).map(|i| compute_hash(&[i])).collect();
        let target = compute_hash(&[3]);
        assert_eq!(find_duplicate(&target, &hashes), Some(3));
    }

    #[test]
    fn find_duplicate_not_found() {
        let hashes: Vec<ChunkHash> = (0..5).map(|i| compute_hash(&[i])).collect();
        let target = compute_hash(&[99]);
        assert_eq!(find_duplicate(&target, &hashes), None);
    }
}
