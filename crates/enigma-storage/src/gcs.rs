#[cfg(feature = "gcs")]
mod inner {
    use async_trait::async_trait;
    use google_cloud_storage::client::{Client, ClientConfig};
    use google_cloud_storage::http::objects::delete::DeleteObjectRequest;
    use google_cloud_storage::http::objects::download::Range;
    use google_cloud_storage::http::objects::get::GetObjectRequest;
    use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};

    use crate::provider::StorageProvider;

    /// Google Cloud Storage provider.
    pub struct GcsStorageProvider {
        client: Client,
        bucket: String,
        name: String,
    }

    impl GcsStorageProvider {
        /// Create using application default credentials.
        pub async fn new(bucket: &str, name: &str) -> anyhow::Result<Self> {
            let config = ClientConfig::default().with_auth().await?;
            let client = Client::new(config);

            Ok(Self {
                client,
                bucket: bucket.to_string(),
                name: name.to_string(),
            })
        }
    }

    #[async_trait]
    impl StorageProvider for GcsStorageProvider {
        async fn upload_chunk(&self, key: &str, data: &[u8]) -> anyhow::Result<()> {
            let upload_type = UploadType::Simple(Media::new(key.to_string()));
            self.client
                .upload_object(
                    &UploadObjectRequest {
                        bucket: self.bucket.clone(),
                        ..Default::default()
                    },
                    data.to_vec(),
                    &upload_type,
                )
                .await?;
            Ok(())
        }

        async fn download_chunk(&self, key: &str) -> anyhow::Result<Vec<u8>> {
            let data = self
                .client
                .download_object(
                    &GetObjectRequest {
                        bucket: self.bucket.clone(),
                        object: key.to_string(),
                        ..Default::default()
                    },
                    &Range::default(),
                )
                .await?;
            Ok(data)
        }

        async fn delete_chunk(&self, key: &str) -> anyhow::Result<()> {
            self.client
                .delete_object(&DeleteObjectRequest {
                    bucket: self.bucket.clone(),
                    object: key.to_string(),
                    ..Default::default()
                })
                .await?;
            Ok(())
        }

        async fn chunk_exists(&self, key: &str) -> anyhow::Result<bool> {
            match self
                .client
                .get_object(&GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: key.to_string(),
                    ..Default::default()
                })
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
            // List objects with max_results=1 to verify connectivity
            use google_cloud_storage::http::objects::list::ListObjectsRequest;
            self.client
                .list_objects(&ListObjectsRequest {
                    bucket: self.bucket.clone(),
                    max_results: Some(1),
                    ..Default::default()
                })
                .await?;
            Ok(())
        }

        fn name(&self) -> &str {
            &self.name
        }
    }
}

#[cfg(feature = "gcs")]
pub use inner::GcsStorageProvider;
