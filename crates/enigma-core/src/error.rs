use thiserror::Error;

#[derive(Debug, Error)]
pub enum EnigmaError {
    // IO
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    // Config
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Configuration file not found at {0} — run `enigma init` first")]
    ConfigNotFound(String),

    // Crypto
    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Decryption error: {0}")]
    Decryption(String),

    #[error("Key not found: {0}")]
    KeyNotFound(String),

    // Compression
    #[error("Compression error: {0}")]
    Compression(String),

    // Chunking
    #[error("Chunking error: {0}")]
    Chunking(String),

    // Storage
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Invalid provider type: {0}")]
    InvalidProviderType(String),

    // Database
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    // Backup
    #[error("Backup not found: {0}")]
    BackupNotFound(String),

    #[error("Invalid status: {0}")]
    InvalidStatus(String),

    // Integrity
    #[error("Hash mismatch for chunk {0}: expected {1}, got {2}")]
    HashMismatch(String, String, String),

    // Serialization
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDe(String),

    #[error("TOML serialization error: {0}")]
    TomlSer(String),
}

pub type Result<T> = std::result::Result<T, EnigmaError>;
