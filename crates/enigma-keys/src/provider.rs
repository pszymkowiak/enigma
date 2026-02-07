use async_trait::async_trait;
use zeroize::ZeroizeOnDrop;

/// A 256-bit encryption key with metadata.
#[derive(Clone, ZeroizeOnDrop)]
pub struct ManagedKey {
    #[zeroize(skip)]
    pub id: String,
    pub key: [u8; 32],
    #[zeroize(skip)]
    pub created_at: String,
}

impl std::fmt::Debug for ManagedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedKey")
            .field("id", &self.id)
            .field("key", &"[REDACTED]")
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Trait for key management backends.
#[async_trait]
pub trait KeyProvider: Send + Sync {
    /// Get the current active encryption key.
    async fn get_current_key(&self) -> anyhow::Result<ManagedKey>;

    /// Get a specific key by ID (for decrypting old chunks).
    async fn get_key_by_id(&self, id: &str) -> anyhow::Result<ManagedKey>;

    /// Create a new encryption key and make it current.
    async fn create_key(&mut self) -> anyhow::Result<ManagedKey>;

    /// Rotate to a new key. Returns the new key.
    async fn rotate_key(&mut self) -> anyhow::Result<ManagedKey>;

    /// List all key IDs.
    async fn list_key_ids(&self) -> anyhow::Result<Vec<String>>;
}
