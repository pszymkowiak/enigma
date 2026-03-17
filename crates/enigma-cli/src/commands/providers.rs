use std::collections::HashMap;
use std::path::Path;

use enigma_core::config::ProviderConfig;
use enigma_core::manifest::ManifestDb;
use enigma_core::types::ProviderType;
use enigma_storage::local::LocalStorageProvider;
use enigma_storage::provider::StorageProvider;
use enigma_storage::s3::S3StorageProvider;

/// Initialize storage providers from config, register them in the DB, and test connections.
pub async fn init_providers(
    provider_configs: &[ProviderConfig],
    db: &ManifestDb,
) -> anyhow::Result<HashMap<i64, Box<dyn StorageProvider>>> {
    let mut storage_providers: HashMap<i64, Box<dyn StorageProvider>> = HashMap::new();

    for pc in provider_configs {
        let existing = db.list_providers()?;
        let pid = match existing.iter().find(|p| p.name == pc.name) {
            Some(p) => p.id,
            None => db.insert_provider(
                &pc.name,
                pc.provider_type,
                &pc.bucket,
                pc.region.as_deref(),
                pc.weight,
            )?,
        };

        let provider: Box<dyn StorageProvider> = match pc.provider_type {
            ProviderType::Local => {
                Box::new(LocalStorageProvider::new(Path::new(&pc.bucket), &pc.name)?)
            }
            ProviderType::S3 => {
                Box::new(S3StorageProvider::new(&pc.bucket, pc.region.as_deref(), &pc.name).await?)
            }
            ProviderType::S3Compatible => {
                let endpoint = pc.endpoint_url.as_deref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Provider '{}': S3Compatible requires 'endpoint_url'",
                        pc.name
                    )
                })?;
                Box::new(
                    S3StorageProvider::s3_compatible(
                        &pc.bucket,
                        endpoint,
                        pc.region.as_deref(),
                        &pc.name,
                        pc.access_key.as_deref(),
                        pc.secret_key.as_deref(),
                    )
                    .await?,
                )
            }
            ProviderType::Azure => {
                anyhow::bail!(
                    "Azure provider '{}' not yet wired in CLI — coming soon.",
                    pc.name
                );
            }
            ProviderType::Gcs => {
                anyhow::bail!(
                    "GCS provider '{}' not yet wired in CLI — coming soon.",
                    pc.name
                );
            }
        };

        provider.test_connection().await?;
        storage_providers.insert(pid, provider);
    }

    Ok(storage_providers)
}
