use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// SHA-256 hash of a chunk's plaintext content.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkHash(pub [u8; 32]);

impl ChunkHash {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hex-encoded hash string.
    pub fn to_hex(&self) -> String {
        hex_encode(&self.0)
    }

    /// Storage key path: `enigma/chunks/ab/cd/{full_hex}`
    pub fn storage_key(&self) -> String {
        let hex = self.to_hex();
        format!("enigma/chunks/{}/{}/{}", &hex[..2], &hex[2..4], &hex)
    }
}

impl fmt::Debug for ChunkHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChunkHash({})", &self.to_hex()[..16])
    }
}

impl fmt::Display for ChunkHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// A raw chunk produced by the chunking engine (plaintext).
#[derive(Debug)]
pub struct RawChunk {
    pub data: Vec<u8>,
    pub hash: ChunkHash,
    pub offset: u64,
    pub length: usize,
}

/// An encrypted chunk ready for upload.
pub struct EncryptedChunk {
    pub hash: ChunkHash,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
    pub key_id: String,
}

impl fmt::Debug for EncryptedChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncryptedChunk")
            .field("hash", &self.hash)
            .field("ciphertext_len", &self.ciphertext.len())
            .finish()
    }
}

/// Encryption key material — zeroized on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct KeyMaterial {
    pub id: String,
    pub key: [u8; 32],
}

impl fmt::Debug for KeyMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyMaterial")
            .field("id", &self.id)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

/// Provider identity used in distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: i64,
    pub name: String,
    pub provider_type: ProviderType,
    pub bucket: String,
    pub region: Option<String>,
    pub weight: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    Local,
    S3,
    /// S3-compatible: MinIO, RustFS, Garage, Ceph RGW, SeaweedFS, etc.
    S3Compatible,
    Azure,
    Gcs,
}

impl fmt::Display for ProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderType::Local => write!(f, "local"),
            ProviderType::S3 => write!(f, "s3"),
            ProviderType::S3Compatible => write!(f, "s3compatible"),
            ProviderType::Azure => write!(f, "azure"),
            ProviderType::Gcs => write!(f, "gcs"),
        }
    }
}

impl std::str::FromStr for ProviderType {
    type Err = crate::error::EnigmaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(ProviderType::Local),
            "s3" => Ok(ProviderType::S3),
            "s3compatible" | "s3-compatible" | "minio" | "rustfs" | "garage" => {
                Ok(ProviderType::S3Compatible)
            }
            "azure" => Ok(ProviderType::Azure),
            "gcs" => Ok(ProviderType::Gcs),
            _ => Err(crate::error::EnigmaError::InvalidProviderType(
                s.to_string(),
            )),
        }
    }
}

/// Backup status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupStatus {
    InProgress,
    Completed,
    Failed,
}

impl fmt::Display for BackupStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackupStatus::InProgress => write!(f, "in_progress"),
            BackupStatus::Completed => write!(f, "completed"),
            BackupStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for BackupStatus {
    type Err = crate::error::EnigmaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "in_progress" => Ok(BackupStatus::InProgress),
            "completed" => Ok(BackupStatus::Completed),
            "failed" => Ok(BackupStatus::Failed),
            _ => Err(crate::error::EnigmaError::InvalidStatus(s.to_string())),
        }
    }
}

/// Summary of a backup run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRecord {
    pub id: String,
    pub source_path: String,
    pub status: BackupStatus,
    pub total_files: u64,
    pub total_bytes: u64,
    pub total_chunks: u64,
    pub dedup_chunks: u64,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Chunking strategy selection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ChunkStrategy {
    Cdc { target_size: u32 },
    Fixed { size: usize },
}

impl Default for ChunkStrategy {
    fn default() -> Self {
        ChunkStrategy::Cdc {
            target_size: 4 * 1024 * 1024, // 4 MB
        }
    }
}

/// Distribution strategy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DistributionStrategy {
    RoundRobin,
    Weighted,
}

impl Default for DistributionStrategy {
    fn default() -> Self {
        DistributionStrategy::RoundRobin
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_hash_hex_roundtrip() {
        let hash = ChunkHash([0xab; 32]);
        let hex = hash.to_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn chunk_hash_storage_key_format() {
        let hash = ChunkHash([0xa3; 32]);
        let key = hash.storage_key();
        assert!(key.starts_with("enigma/chunks/a3/a3/"));
    }

    #[test]
    fn provider_type_parse() {
        assert_eq!("s3".parse::<ProviderType>().unwrap(), ProviderType::S3);
        assert_eq!(
            "azure".parse::<ProviderType>().unwrap(),
            ProviderType::Azure
        );
        assert_eq!(
            "s3compatible".parse::<ProviderType>().unwrap(),
            ProviderType::S3Compatible
        );
        assert_eq!(
            "minio".parse::<ProviderType>().unwrap(),
            ProviderType::S3Compatible
        );
        assert_eq!(
            "rustfs".parse::<ProviderType>().unwrap(),
            ProviderType::S3Compatible
        );
        assert_eq!(
            "garage".parse::<ProviderType>().unwrap(),
            ProviderType::S3Compatible
        );
        assert!("invalid".parse::<ProviderType>().is_err());
    }

    #[test]
    fn key_material_zeroize_on_drop() {
        let key = KeyMaterial {
            id: "test".to_string(),
            key: [0x42; 32],
        };
        let debug = format!("{key:?}");
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("42"));
    }
}
