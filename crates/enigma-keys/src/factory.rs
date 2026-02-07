//! Factory for creating the appropriate KeyProvider based on configuration.

use std::path::Path;

use crate::local::LocalKeyProvider;
use crate::provider::KeyProvider;

/// Create a KeyProvider based on the provider type string from config.
///
/// Supported types:
/// - `"local"` — file-based encrypted keyfile (requires passphrase + keyfile_path)
/// - `"azure-keyvault"` — Azure Key Vault (requires vault_url, compile with `azure-keyvault` feature)
/// - `"gcp-secretmanager"` — GCP Secret Manager (requires gcp_project_id, compile with `gcp-secretmanager` feature)
/// - `"aws-secretsmanager"` — AWS Secrets Manager (requires aws_region, compile with `aws-secretsmanager` feature)
#[allow(unused_variables)]
pub async fn create_key_provider(
    provider_type: &str,
    passphrase: Option<&[u8]>,
    keyfile_path: &str,
    vault_url: Option<&str>,
    gcp_project_id: Option<&str>,
    aws_region: Option<&str>,
    secret_prefix: Option<&str>,
) -> anyhow::Result<Box<dyn KeyProvider>> {
    match provider_type {
        "local" => {
            let passphrase = passphrase
                .ok_or_else(|| anyhow::anyhow!("Passphrase required for local key provider"))?;
            let path = Path::new(keyfile_path);
            let provider = if path.exists() {
                LocalKeyProvider::open(path, passphrase)?
            } else {
                LocalKeyProvider::create(path, passphrase)?
            };
            Ok(Box::new(provider))
        }

        #[cfg(feature = "azure-keyvault")]
        "azure-keyvault" => {
            let url = vault_url
                .ok_or_else(|| anyhow::anyhow!("vault_url required for azure-keyvault provider"))?;
            let provider = crate::azure_vault::AzureKeyVaultProvider::new(
                url,
                secret_prefix.or(Some("enigma-key")),
            )?;
            Ok(Box::new(provider))
        }

        #[cfg(not(feature = "azure-keyvault"))]
        "azure-keyvault" => {
            anyhow::bail!(
                "azure-keyvault feature not enabled. Recompile with --features azure-keyvault"
            )
        }

        #[cfg(feature = "gcp-secretmanager")]
        "gcp-secretmanager" => {
            let project_id = gcp_project_id.ok_or_else(|| {
                anyhow::anyhow!("gcp_project_id required for gcp-secretmanager provider")
            })?;
            let provider = crate::gcp_secretmanager::GcpSecretManagerProvider::new(
                project_id,
                secret_prefix.or(Some("enigma-key")),
            )
            .await?;
            Ok(Box::new(provider))
        }

        #[cfg(not(feature = "gcp-secretmanager"))]
        "gcp-secretmanager" => {
            anyhow::bail!(
                "gcp-secretmanager feature not enabled. Recompile with --features gcp-secretmanager"
            )
        }

        #[cfg(feature = "aws-secretsmanager")]
        "aws-secretsmanager" => {
            let region = aws_region.ok_or_else(|| {
                anyhow::anyhow!("aws_region required for aws-secretsmanager provider")
            })?;
            let provider = crate::aws_secretsmanager::AwsSecretManagerProvider::new(
                region,
                secret_prefix.or(Some("enigma-key")),
            )
            .await?;
            Ok(Box::new(provider))
        }

        #[cfg(not(feature = "aws-secretsmanager"))]
        "aws-secretsmanager" => {
            anyhow::bail!(
                "aws-secretsmanager feature not enabled. Recompile with --features aws-secretsmanager"
            )
        }

        other => anyhow::bail!("Unknown key provider type: {other}"),
    }
}
