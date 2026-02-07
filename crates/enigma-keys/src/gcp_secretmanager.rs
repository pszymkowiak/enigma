//! GCP Secret Manager KeyProvider implementation.
//!
//! Stores 32-byte encryption keys as secrets in Google Cloud Secret Manager.
//! Each key = one secret named `{prefix}-{uuid}` with one version containing the raw bytes.
//! A metadata secret `{prefix}-current` tracks the active key ID.

use async_trait::async_trait;
use google_cloud_secretmanager_v1::client::SecretManagerService;
use google_cloud_secretmanager_v1::model::replication::Automatic;
use google_cloud_secretmanager_v1::model::{Replication, Secret, SecretPayload};
use rand::RngCore;
use rand::rngs::OsRng;
use uuid::Uuid;

use crate::provider::{KeyProvider, ManagedKey};

/// GCP Secret Manager key provider.
pub struct GcpSecretManagerProvider {
    client: SecretManagerService,
    project_id: String,
    prefix: String,
}

impl GcpSecretManagerProvider {
    /// Create a new provider connected to GCP Secret Manager.
    ///
    /// Uses Application Default Credentials (`gcloud auth application-default login`).
    pub async fn new(project_id: &str, prefix: Option<&str>) -> anyhow::Result<Self> {
        let client = SecretManagerService::builder()
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("GCP Secret Manager client error: {e}"))?;

        Ok(Self {
            client,
            project_id: project_id.to_string(),
            prefix: prefix.unwrap_or("enigma-key").to_string(),
        })
    }

    fn parent(&self) -> String {
        format!("projects/{}", self.project_id)
    }

    fn secret_id(&self, key_id: &str) -> String {
        format!("{}-{}", self.prefix, key_id)
    }

    fn secret_name(&self, key_id: &str) -> String {
        format!("{}/secrets/{}", self.parent(), self.secret_id(key_id))
    }

    fn meta_secret_id(&self) -> String {
        format!("{}-current", self.prefix)
    }

    fn meta_secret_name(&self) -> String {
        format!("{}/secrets/{}", self.parent(), self.meta_secret_id())
    }

    /// Create a GCP secret, ignoring ALREADY_EXISTS errors.
    async fn ensure_secret(&self, secret_id: &str) -> anyhow::Result<()> {
        let result = self
            .client
            .create_secret()
            .set_parent(&self.parent())
            .set_secret_id(secret_id)
            .set_secret(
                Secret::new()
                    .set_replication(Replication::new().set_automatic(Automatic::default())),
            )
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("ALREADY_EXISTS") {
                    Ok(()) // Secret already exists, fine
                } else {
                    Err(anyhow::anyhow!(
                        "GCP create_secret({secret_id}) failed: {e}"
                    ))
                }
            }
        }
    }

    /// Create a secret and add a version with the key bytes.
    async fn store_key(&self, key_id: &str, key_bytes: &[u8; 32]) -> anyhow::Result<String> {
        let secret_id = self.secret_id(key_id);

        self.ensure_secret(&secret_id).await?;

        // Add version with key payload
        self.client
            .add_secret_version()
            .set_parent(&self.secret_name(key_id))
            .set_payload(SecretPayload::new().set_data(bytes::Bytes::from(key_bytes.to_vec())))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("GCP add_secret_version failed: {e}"))?;

        let created_at = chrono::Utc::now().to_rfc3339();
        Ok(created_at)
    }

    /// Read a key from a secret version.
    async fn read_key(&self, key_id: &str) -> anyhow::Result<ManagedKey> {
        let version_name = format!("{}/versions/latest", self.secret_name(key_id));

        let response = self
            .client
            .access_secret_version()
            .set_name(&version_name)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("GCP access_secret_version({key_id}) failed: {e}"))?;

        let payload = response
            .payload
            .ok_or_else(|| anyhow::anyhow!("Secret {key_id} has no payload"))?;

        let data = payload.data;
        if data.len() != 32 {
            anyhow::bail!("Secret {key_id}: expected 32 bytes, got {}", data.len());
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&data);

        let created_at = chrono::Utc::now().to_rfc3339();

        Ok(ManagedKey {
            id: key_id.to_string(),
            key,
            created_at,
        })
    }

    /// Get the current key ID from metadata secret.
    async fn get_current_key_id(&self) -> anyhow::Result<String> {
        let version_name = format!("{}/versions/latest", self.meta_secret_name());

        let response = self
            .client
            .access_secret_version()
            .set_name(&version_name)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("GCP get current key ID failed: {e}"))?;

        let payload = response
            .payload
            .ok_or_else(|| anyhow::anyhow!("Metadata secret has no payload"))?;

        Ok(String::from_utf8(payload.data.to_vec())?)
    }

    /// Set the current key ID in metadata secret (create-if-needed + add version).
    async fn set_current_key_id(&self, key_id: &str) -> anyhow::Result<()> {
        self.ensure_secret(&self.meta_secret_id()).await?;

        self.client
            .add_secret_version()
            .set_parent(&self.meta_secret_name())
            .set_payload(
                SecretPayload::new().set_data(bytes::Bytes::from(key_id.as_bytes().to_vec())),
            )
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("GCP set current key ID failed: {e}"))?;

        Ok(())
    }
}

#[async_trait]
impl KeyProvider for GcpSecretManagerProvider {
    async fn get_current_key(&self) -> anyhow::Result<ManagedKey> {
        let key_id = self.get_current_key_id().await?;
        self.read_key(&key_id).await
    }

    async fn get_key_by_id(&self, id: &str) -> anyhow::Result<ManagedKey> {
        self.read_key(id).await
    }

    async fn create_key(&mut self) -> anyhow::Result<ManagedKey> {
        let key_id = Uuid::now_v7().to_string();
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);

        let created_at = self.store_key(&key_id, &key_bytes).await?;
        self.set_current_key_id(&key_id).await?;

        tracing::info!(key_id = %key_id, "Created new key in GCP Secret Manager");

        Ok(ManagedKey {
            id: key_id,
            key: key_bytes,
            created_at,
        })
    }

    async fn rotate_key(&mut self) -> anyhow::Result<ManagedKey> {
        self.create_key().await
    }

    async fn list_key_ids(&self) -> anyhow::Result<Vec<String>> {
        use google_cloud_gax::paginator::ItemPaginator as _;

        let prefix_dash = format!("{}-", self.prefix);
        let meta_id = self.meta_secret_id();
        let mut ids = Vec::new();
        let mut items = self
            .client
            .list_secrets()
            .set_parent(&self.parent())
            .by_item();

        while let Some(item) = items.next().await {
            let secret = item.map_err(|e| anyhow::anyhow!("GCP list error: {e}"))?;
            if let Some(sid) = secret.name.rsplit('/').next() {
                if sid.starts_with(&prefix_dash) && sid != meta_id {
                    let key_id = sid.strip_prefix(&prefix_dash).unwrap_or(sid).to_string();
                    ids.push(key_id);
                }
            }
        }

        Ok(ids)
    }
}
