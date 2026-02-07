#[cfg(feature = "azure")]
mod inner {
    use async_trait::async_trait;
    use azure_storage::StorageCredentials;
    use azure_storage_blobs::prelude::*;

    use crate::provider::StorageProvider;

    /// Azure Blob Storage provider.
    pub struct AzureStorageProvider {
        container_client: ContainerClient,
        name: String,
    }

    impl AzureStorageProvider {
        /// Create from storage account name + access key.
        pub fn new(
            account: &str,
            access_key: &str,
            container: &str,
            name: &str,
        ) -> anyhow::Result<Self> {
            let credentials = StorageCredentials::access_key(account, access_key.to_string());
            let container_client =
                ClientBuilder::new(account, credentials).container_client(container);

            Ok(Self {
                container_client,
                name: name.to_string(),
            })
        }

        /// Create using the emulator (Azurite).
        pub fn emulator(container: &str, name: &str) -> anyhow::Result<Self> {
            let container_client = ClientBuilder::emulator().container_client(container);

            Ok(Self {
                container_client,
                name: name.to_string(),
            })
        }
    }

    #[async_trait]
    impl StorageProvider for AzureStorageProvider {
        async fn upload_chunk(&self, key: &str, data: &[u8]) -> anyhow::Result<()> {
            self.container_client
                .blob_client(key)
                .put_block_blob(data.to_vec())
                .await?;
            Ok(())
        }

        async fn download_chunk(&self, key: &str) -> anyhow::Result<Vec<u8>> {
            let resp = self.container_client.blob_client(key).get_content().await?;
            Ok(resp)
        }

        async fn delete_chunk(&self, key: &str) -> anyhow::Result<()> {
            self.container_client.blob_client(key).delete().await?;
            Ok(())
        }

        async fn chunk_exists(&self, key: &str) -> anyhow::Result<bool> {
            match self
                .container_client
                .blob_client(key)
                .get_properties()
                .await
            {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        }

        async fn upload_manifest(&self, data: &[u8]) -> anyhow::Result<()> {
            self.upload_chunk("enigma-manifest.enc", data).await
        }

        async fn download_manifest(&self) -> anyhow::Result<Vec<u8>> {
            self.download_chunk("enigma-manifest.enc").await
        }

        async fn test_connection(&self) -> anyhow::Result<()> {
            self.container_client.get_properties().await?;
            Ok(())
        }

        fn name(&self) -> &str {
            &self.name
        }
    }
}

#[cfg(feature = "azure")]
pub use inner::AzureStorageProvider;
