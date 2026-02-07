use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::provider::StorageProvider;

/// Filesystem-based storage provider for local testing.
pub struct LocalStorageProvider {
    base_path: PathBuf,
    name: String,
}

impl LocalStorageProvider {
    pub fn new(base_path: &Path, name: &str) -> anyhow::Result<Self> {
        std::fs::create_dir_all(base_path)?;
        Ok(Self {
            base_path: base_path.to_path_buf(),
            name: name.to_string(),
        })
    }

    fn chunk_path(&self, key: &str) -> PathBuf {
        self.base_path.join(key)
    }

    fn manifest_path(&self) -> PathBuf {
        self.base_path.join("enigma-manifest.enc")
    }
}

#[async_trait]
impl StorageProvider for LocalStorageProvider {
    async fn upload_chunk(&self, key: &str, data: &[u8]) -> anyhow::Result<()> {
        let path = self.chunk_path(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data)?;
        Ok(())
    }

    async fn download_chunk(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        let path = self.chunk_path(key);
        Ok(std::fs::read(&path)?)
    }

    async fn delete_chunk(&self, key: &str) -> anyhow::Result<()> {
        let path = self.chunk_path(key);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    async fn chunk_exists(&self, key: &str) -> anyhow::Result<bool> {
        Ok(self.chunk_path(key).exists())
    }

    async fn upload_manifest(&self, data: &[u8]) -> anyhow::Result<()> {
        std::fs::write(self.manifest_path(), data)?;
        Ok(())
    }

    async fn download_manifest(&self) -> anyhow::Result<Vec<u8>> {
        Ok(std::fs::read(self.manifest_path())?)
    }

    async fn test_connection(&self) -> anyhow::Result<()> {
        if !self.base_path.exists() {
            anyhow::bail!("Base path does not exist: {}", self.base_path.display());
        }
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn upload_download_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let provider = LocalStorageProvider::new(tmp.path(), "test-local").unwrap();

        let data = b"encrypted chunk data here";
        let key = "enigma/chunks/ab/cd/deadbeef";

        provider.upload_chunk(key, data).await.unwrap();
        assert!(provider.chunk_exists(key).await.unwrap());

        let downloaded = provider.download_chunk(key).await.unwrap();
        assert_eq!(downloaded, data);

        provider.delete_chunk(key).await.unwrap();
        assert!(!provider.chunk_exists(key).await.unwrap());
    }

    #[tokio::test]
    async fn manifest_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let provider = LocalStorageProvider::new(tmp.path(), "test-local").unwrap();

        let data = b"encrypted manifest";
        provider.upload_manifest(data).await.unwrap();
        let downloaded = provider.download_manifest().await.unwrap();
        assert_eq!(downloaded, data);
    }

    #[tokio::test]
    async fn test_connection_ok() {
        let tmp = TempDir::new().unwrap();
        let provider = LocalStorageProvider::new(tmp.path(), "test-local").unwrap();
        provider.test_connection().await.unwrap();
    }
}
