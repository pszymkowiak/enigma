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

    fn chunk_path(&self, key: &str) -> anyhow::Result<PathBuf> {
        // Reject path traversal
        if key.contains("..") || key.starts_with('/') || key.starts_with('\\') {
            anyhow::bail!("invalid chunk key: path traversal detected");
        }
        Ok(self.base_path.join(key))
    }
}

#[async_trait]
impl StorageProvider for LocalStorageProvider {
    async fn upload_chunk(&self, key: &str, data: &[u8]) -> anyhow::Result<()> {
        let path = self.chunk_path(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, data).await?;
        Ok(())
    }

    async fn download_chunk(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        let path = self.chunk_path(key)?;
        Ok(tokio::fs::read(&path).await?)
    }

    async fn delete_chunk(&self, key: &str) -> anyhow::Result<()> {
        let path = self.chunk_path(key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn chunk_exists(&self, key: &str) -> anyhow::Result<bool> {
        let path = self.chunk_path(key)?;
        Ok(tokio::fs::try_exists(&path).await?)
    }

    async fn test_connection(&self) -> anyhow::Result<()> {
        if !tokio::fs::try_exists(&self.base_path).await? {
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

    #[tokio::test]
    async fn rejects_path_traversal() {
        let tmp = TempDir::new().unwrap();
        let provider = LocalStorageProvider::new(tmp.path(), "test-local").unwrap();

        assert!(provider.upload_chunk("../etc/passwd", b"x").await.is_err());
        assert!(provider.upload_chunk("/etc/passwd", b"x").await.is_err());
        assert!(
            provider
                .upload_chunk("\\windows\\system32", b"x")
                .await
                .is_err()
        );
        assert!(
            provider
                .upload_chunk("foo/../../etc/passwd", b"x")
                .await
                .is_err()
        );
    }
}
