//! AWS Secrets Manager KeyProvider implementation.
//!
//! Stores 32-byte encryption keys as base64 secrets in AWS Secrets Manager.
//! Each key = one secret named `{prefix}-{uuid}` with the raw key bytes (base64-encoded).
//! A metadata secret `{prefix}-current` tracks the active key ID.

use async_trait::async_trait;
use aws_sdk_secretsmanager::Client;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use rand::RngCore;
use rand::rngs::OsRng;
use uuid::Uuid;

use crate::provider::{KeyProvider, ManagedKey};

/// AWS Secrets Manager key provider.
pub struct AwsSecretManagerProvider {
    client: Client,
    prefix: String,
    region: String,
}

impl AwsSecretManagerProvider {
    /// Create a new provider connected to AWS Secrets Manager.
    ///
    /// Uses default credential chain (env vars, AWS CLI profile, IAM role, etc.).
    pub async fn new(region: &str, prefix: Option<&str>) -> anyhow::Result<Self> {
        let region_provider = aws_config::Region::new(region.to_string());
        let config = aws_config::from_env().region(region_provider).load().await;
        let client = Client::new(&config);

        Ok(Self {
            client,
            prefix: prefix.unwrap_or("enigma-key").to_string(),
            region: region.to_string(),
        })
    }

    fn secret_name(&self, key_id: &str) -> String {
        format!("{}-{}", self.prefix, key_id)
    }

    fn meta_secret_name(&self) -> String {
        format!("{}-current", self.prefix)
    }

    /// Create a secret, ignoring ResourceExistsException.
    async fn ensure_secret(&self, name: &str) -> anyhow::Result<()> {
        let result = self
            .client
            .create_secret()
            .name(name)
            .secret_string("")
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("ResourceExistsException") {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("AWS create_secret({name}) failed: {e}"))
                }
            }
        }
    }

    /// Store a 32-byte key in a secret.
    async fn store_key(&self, key_id: &str, key_bytes: &[u8; 32]) -> anyhow::Result<String> {
        let name = self.secret_name(key_id);
        let encoded = BASE64.encode(key_bytes);

        self.ensure_secret(&name).await?;

        self.client
            .put_secret_value()
            .secret_id(&name)
            .secret_string(&encoded)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("AWS put_secret_value({name}) failed: {e}"))?;

        let created_at = chrono::Utc::now().to_rfc3339();
        Ok(created_at)
    }

    /// Read a key from a secret.
    async fn read_key(&self, key_id: &str) -> anyhow::Result<ManagedKey> {
        let name = self.secret_name(key_id);

        let resp = self
            .client
            .get_secret_value()
            .secret_id(&name)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("AWS get_secret_value({name}) failed: {e}"))?;

        let value = resp
            .secret_string()
            .ok_or_else(|| anyhow::anyhow!("Secret {name} has no string value"))?;

        let key_bytes = BASE64.decode(value)?;
        if key_bytes.len() != 32 {
            anyhow::bail!("Secret {name}: expected 32 bytes, got {}", key_bytes.len());
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);

        let created_at = resp
            .created_date()
            .map(|d| d.to_string())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        Ok(ManagedKey {
            id: key_id.to_string(),
            key,
            created_at,
        })
    }

    /// Get the current key ID from metadata secret.
    async fn get_current_key_id(&self) -> anyhow::Result<String> {
        let name = self.meta_secret_name();

        let resp = self
            .client
            .get_secret_value()
            .secret_id(&name)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("AWS get current key ID failed: {e}"))?;

        resp.secret_string()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Metadata secret has no value"))
    }

    /// Set the current key ID in the metadata secret.
    async fn set_current_key_id(&self, key_id: &str) -> anyhow::Result<()> {
        let name = self.meta_secret_name();

        self.ensure_secret(&name).await?;

        self.client
            .put_secret_value()
            .secret_id(&name)
            .secret_string(key_id)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("AWS set current key ID failed: {e}"))?;

        Ok(())
    }

    /// Region used by this provider.
    pub fn region(&self) -> &str {
        &self.region
    }
}

#[async_trait]
impl KeyProvider for AwsSecretManagerProvider {
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

        tracing::info!(key_id = %key_id, "Created new key in AWS Secrets Manager");

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
        let prefix_dash = format!("{}-", self.prefix);
        let meta_name = self.meta_secret_name();
        let mut ids = Vec::new();

        let mut paginator = self
            .client
            .list_secrets()
            .filters(
                aws_sdk_secretsmanager::types::Filter::builder()
                    .key(aws_sdk_secretsmanager::types::FilterNameStringType::Name)
                    .values(&prefix_dash)
                    .build(),
            )
            .into_paginator()
            .send();

        while let Some(page) = paginator.next().await {
            let page = page.map_err(|e| anyhow::anyhow!("AWS list_secrets error: {e}"))?;
            for secret in page.secret_list() {
                if let Some(name) = &secret.name {
                    if name.starts_with(&prefix_dash) && *name != meta_name {
                        let key_id = name.strip_prefix(&prefix_dash).unwrap_or(name).to_string();
                        ids.push(key_id);
                    }
                }
            }
        }

        Ok(ids)
    }
}
