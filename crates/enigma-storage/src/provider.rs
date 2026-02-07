use async_trait::async_trait;

/// Trait for cloud/local storage backends.
#[async_trait]
pub trait StorageProvider: Send + Sync {
    /// Upload an encrypted chunk.
    async fn upload_chunk(&self, key: &str, data: &[u8]) -> anyhow::Result<()>;

    /// Download an encrypted chunk.
    async fn download_chunk(&self, key: &str) -> anyhow::Result<Vec<u8>>;

    /// Delete an encrypted chunk.
    async fn delete_chunk(&self, key: &str) -> anyhow::Result<()>;

    /// Check if a chunk exists.
    async fn chunk_exists(&self, key: &str) -> anyhow::Result<bool>;

    /// Upload manifest data.
    async fn upload_manifest(&self, data: &[u8]) -> anyhow::Result<()>;

    /// Download manifest data.
    async fn download_manifest(&self) -> anyhow::Result<Vec<u8>>;

    /// Test connectivity.
    async fn test_connection(&self) -> anyhow::Result<()>;

    /// Provider name for display.
    fn name(&self) -> &str;
}
