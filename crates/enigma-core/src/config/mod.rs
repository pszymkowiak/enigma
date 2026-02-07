pub mod credentials;

use crate::error::{EnigmaError, Result};
use crate::types::{ChunkStrategy, DistributionStrategy, ProviderType};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Top-level Enigma configuration stored as TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnigmaConfig {
    pub enigma: EnigmaSettings,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnigmaSettings {
    /// Path to the SQLite database (manifest + logs).
    pub db_path: String,
    /// Chunking strategy.
    #[serde(default)]
    pub chunk_strategy: ChunkStrategy,
    /// Distribution strategy.
    #[serde(default)]
    pub distribution: DistributionStrategy,
    /// Key provider type ("local" or "vault").
    #[serde(default = "default_key_provider")]
    pub key_provider: String,
    /// Path to the encrypted keyfile (for local key provider).
    #[serde(default = "default_keyfile_path")]
    pub keyfile_path: String,
    /// Compression settings.
    #[serde(default)]
    pub compression: CompressionConfig,
    /// Number of providers each chunk is replicated to (default: 1 = no replication).
    #[serde(default = "default_replication_factor")]
    pub replication_factor: u32,
    /// Azure Key Vault URL (for key_provider = "azure-keyvault").
    #[serde(default)]
    pub vault_url: Option<String>,
    /// GCP project ID (for key_provider = "gcp-secretmanager").
    #[serde(default)]
    pub gcp_project_id: Option<String>,
    /// AWS region (for key_provider = "aws-secretsmanager").
    #[serde(default)]
    pub aws_region: Option<String>,
    /// Secret name prefix used in vault backends (default: "enigma-key").
    #[serde(default)]
    pub secret_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    pub enabled: bool,
    pub level: i32,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            level: 3,
        }
    }
}

fn default_replication_factor() -> u32 {
    1
}

fn default_key_provider() -> String {
    "local".to_string()
}

fn default_keyfile_path() -> String {
    "keys.enc".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    pub bucket: String,
    pub region: Option<String>,
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// Custom endpoint URL for S3-compatible providers (MinIO, RustFS, Garage, etc.)
    #[serde(default)]
    pub endpoint_url: Option<String>,
    /// Use path-style addressing (required by most S3-compatible servers).
    /// Default: true for S3Compatible, false for S3.
    #[serde(default)]
    pub path_style: Option<bool>,
    /// S3 access key (for S3/S3Compatible providers).
    #[serde(default)]
    pub access_key: Option<String>,
    /// S3 secret key (for S3/S3Compatible providers).
    #[serde(default)]
    pub secret_key: Option<String>,
    /// Credential reference — either inline encrypted or a vault path.
    #[serde(default)]
    pub credential_ref: Option<String>,
}

fn default_weight() -> u32 {
    1
}

impl EnigmaConfig {
    /// Load config from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(EnigmaError::ConfigNotFound(path.display().to_string()));
        }
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| EnigmaError::TomlDe(e.to_string()))
    }

    /// Save config to a TOML file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content =
            toml::to_string_pretty(self).map_err(|e| EnigmaError::TomlSer(e.to_string()))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Default config for `enigma init`.
    pub fn default_config(base_dir: &Path) -> Self {
        Self {
            enigma: EnigmaSettings {
                db_path: base_dir.join("enigma.db").display().to_string(),
                chunk_strategy: ChunkStrategy::default(),
                distribution: DistributionStrategy::default(),
                key_provider: "local".to_string(),
                keyfile_path: base_dir.join("keys.enc").display().to_string(),
                compression: CompressionConfig::default(),
                replication_factor: 1,
                vault_url: None,
                gcp_project_id: None,
                aws_region: None,
                secret_prefix: None,
            },
            providers: vec![],
        }
    }

    /// Resolve the config file path: `<base_dir>/enigma.toml`
    pub fn default_path(base_dir: &Path) -> PathBuf {
        base_dir.join("enigma.toml")
    }

    /// Resolve the default enigma home directory: `~/.enigma`
    pub fn default_base_dir() -> Result<PathBuf> {
        dirs::home_dir()
            .map(|h| h.join(".enigma"))
            .ok_or_else(|| EnigmaError::Config("Cannot determine home directory".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn roundtrip_config() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("enigma.toml");
        let config = EnigmaConfig::default_config(tmp.path());
        config.save(&path).unwrap();
        let loaded = EnigmaConfig::load(&path).unwrap();
        assert_eq!(loaded.enigma.key_provider, "local");
        assert!(loaded.providers.is_empty());
    }

    #[test]
    fn load_nonexistent_returns_error() {
        let result = EnigmaConfig::load(Path::new("/nonexistent/enigma.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn roundtrip_with_replication_factor() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("enigma.toml");
        let mut config = EnigmaConfig::default_config(tmp.path());
        config.enigma.replication_factor = 3;
        config.save(&path).unwrap();
        let loaded = EnigmaConfig::load(&path).unwrap();
        assert_eq!(loaded.enigma.replication_factor, 3);
    }

    #[test]
    fn default_replication_factor_is_one() {
        let tmp = TempDir::new().unwrap();
        let config = EnigmaConfig::default_config(tmp.path());
        assert_eq!(config.enigma.replication_factor, 1);
    }
}
