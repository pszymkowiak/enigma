//! Azure Key Vault KeyProvider implementation.
//!
//! Stores 32-byte encryption keys as base64 secrets in Azure Key Vault.
//! A metadata secret `enigma-current-key` tracks the active key ID.
//!
//! Each key is stored as a secret named `enigma-key-{uuid}`.

use async_trait::async_trait;
use azure_identity::AzureCliCredential;
use azure_security_keyvault_secrets::models::SetSecretParameters;
use azure_security_keyvault_secrets::{ResourceExt, SecretClient};
use futures::TryStreamExt;
use rand::RngCore;
use rand::rngs::OsRng;
use std::collections::HashMap;
use uuid::Uuid;

use crate::provider::{KeyProvider, ManagedKey};

/// Azure Key Vault key provider.
pub struct AzureKeyVaultProvider {
    client: SecretClient,
    prefix: String,
}

const META_SECRET: &str = "enigma-current-key";

impl AzureKeyVaultProvider {
    /// Create a new provider connected to an Azure Key Vault.
    ///
    /// `vault_url` example: `https://my-vault.vault.azure.net/`
    pub fn new(vault_url: &str, prefix: Option<&str>) -> anyhow::Result<Self> {
        let credential = AzureCliCredential::new(None)
            .map_err(|e| anyhow::anyhow!("Azure credential error: {e}"))?;
        let client = SecretClient::new(vault_url, credential, None)
            .map_err(|e| anyhow::anyhow!("Azure KV client error: {e}"))?;

        Ok(Self {
            client,
            prefix: prefix.unwrap_or("enigma-key").to_string(),
        })
    }

    /// Secret name for a key ID.
    fn secret_name(&self, key_id: &str) -> String {
        format!("{}-{}", self.prefix, key_id)
    }

    /// Store a 32-byte key in the vault.
    async fn store_key(&self, key_id: &str, key_bytes: &[u8; 32]) -> anyhow::Result<String> {
        let encoded = base64_encode(key_bytes);
        let name = self.secret_name(key_id);

        let mut tags = HashMap::new();
        tags.insert("enigma-key-id".to_string(), key_id.to_string());
        tags.insert("created-by".to_string(), "enigma".to_string());

        let params = SetSecretParameters {
            value: Some(encoded),
            content_type: Some("application/x-enigma-key".to_string()),
            tags: Some(tags),
            ..Default::default()
        };

        let secret = self
            .client
            .set_secret(&name, params.try_into()?, None)
            .await
            .map_err(|e| anyhow::anyhow!("Azure KV set_secret failed: {e}"))?
            .into_model()
            .map_err(|e| anyhow::anyhow!("Azure KV model error: {e}"))?;

        let created = secret
            .attributes
            .and_then(|a| a.created.map(|t| t.to_string()))
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        Ok(created)
    }

    /// Read a 32-byte key from the vault.
    async fn read_key(&self, key_id: &str) -> anyhow::Result<(ManagedKey, String)> {
        let name = self.secret_name(key_id);

        let secret = self
            .client
            .get_secret(&name, None)
            .await
            .map_err(|e| anyhow::anyhow!("Azure KV get_secret({name}) failed: {e}"))?
            .into_model()
            .map_err(|e| anyhow::anyhow!("Azure KV model error: {e}"))?;

        let value = secret
            .value
            .ok_or_else(|| anyhow::anyhow!("Secret {name} has no value"))?;

        let key_bytes = base64_decode(&value)?;
        if key_bytes.len() != 32 {
            anyhow::bail!("Secret {name}: expected 32 bytes, got {}", key_bytes.len());
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);

        let created_at = secret
            .attributes
            .and_then(|a| a.created.map(|t| t.to_string()))
            .unwrap_or_default();

        Ok((
            ManagedKey {
                id: key_id.to_string(),
                key,
                created_at: created_at.clone(),
            },
            created_at,
        ))
    }

    /// Get the current key ID from metadata secret.
    async fn get_current_key_id(&self) -> anyhow::Result<String> {
        let secret = self
            .client
            .get_secret(META_SECRET, None)
            .await
            .map_err(|e| anyhow::anyhow!("Azure KV get current key ID failed: {e}"))?
            .into_model()
            .map_err(|e| anyhow::anyhow!("Azure KV model error: {e}"))?;

        secret
            .value
            .ok_or_else(|| anyhow::anyhow!("Metadata secret has no value"))
    }

    /// Set the current key ID in metadata secret.
    async fn set_current_key_id(&self, key_id: &str) -> anyhow::Result<()> {
        let params = SetSecretParameters {
            value: Some(key_id.to_string()),
            content_type: Some("text/plain".to_string()),
            ..Default::default()
        };

        self.client
            .set_secret(META_SECRET, params.try_into()?, None)
            .await
            .map_err(|e| anyhow::anyhow!("Azure KV set current key ID failed: {e}"))?;

        Ok(())
    }
}

#[async_trait]
impl KeyProvider for AzureKeyVaultProvider {
    async fn get_current_key(&self) -> anyhow::Result<ManagedKey> {
        let key_id = self.get_current_key_id().await?;
        let (managed_key, _) = self.read_key(&key_id).await?;
        Ok(managed_key)
    }

    async fn get_key_by_id(&self, id: &str) -> anyhow::Result<ManagedKey> {
        let (managed_key, _) = self.read_key(id).await?;
        Ok(managed_key)
    }

    async fn create_key(&mut self) -> anyhow::Result<ManagedKey> {
        let key_id = Uuid::now_v7().to_string();
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);

        let created_at = self.store_key(&key_id, &key_bytes).await?;
        self.set_current_key_id(&key_id).await?;

        tracing::info!(key_id = %key_id, "Created new key in Azure Key Vault");

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
        let mut ids = Vec::new();
        let mut pager = self
            .client
            .list_secret_properties(None)
            .map_err(|e| anyhow::anyhow!("Azure KV list failed: {e}"))?
            .into_stream();

        while let Some(props) = pager
            .try_next()
            .await
            .map_err(|e| anyhow::anyhow!("Azure KV pager error: {e}"))?
        {
            if let Ok(rid) = props.resource_id() {
                let name = rid.name.to_string();
                // Only return enigma key secrets, not the metadata secret
                if name.starts_with(&self.prefix) && name != META_SECRET {
                    let key_id = name
                        .strip_prefix(&format!("{}-", self.prefix))
                        .unwrap_or(&name)
                        .to_string();
                    ids.push(key_id);
                }
            }
        }

        Ok(ids)
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((n >> 18) & 63) as usize] as char);
        result.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(s: &str) -> anyhow::Result<Vec<u8>> {
    fn val(c: u8) -> anyhow::Result<u32> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((c - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            b'=' => Ok(0),
            _ => anyhow::bail!("invalid base64 char: {c}"),
        }
    }
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    for chunk in bytes.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let n = (val(chunk[0])? << 18)
            | (val(chunk[1])? << 12)
            | (val(chunk[2])? << 6)
            | val(chunk[3])?;
        result.push(((n >> 16) & 0xFF) as u8);
        if chunk[2] != b'=' {
            result.push(((n >> 8) & 0xFF) as u8);
        }
        if chunk[3] != b'=' {
            result.push((n & 0xFF) as u8);
        }
    }
    Ok(result)
}
