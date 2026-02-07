#[cfg(feature = "s3")]
mod inner {
    use async_trait::async_trait;
    use aws_sdk_s3::Client;
    use aws_sdk_s3::primitives::ByteStream;

    use crate::provider::StorageProvider;

    /// AWS S3 and S3-compatible storage provider.
    ///
    /// Works with AWS S3, MinIO, RustFS, Garage, Ceph RGW, SeaweedFS,
    /// and any other service implementing the S3 API.
    pub struct S3StorageProvider {
        client: Client,
        bucket: String,
        name: String,
    }

    /// Options for creating an S3 provider.
    pub struct S3Options<'a> {
        pub bucket: &'a str,
        pub region: Option<&'a str>,
        pub name: &'a str,
        /// Custom endpoint URL (e.g. `http://localhost:9000` for MinIO).
        pub endpoint_url: Option<&'a str>,
        /// Force path-style addressing (`http://host/bucket/key` instead of `http://bucket.host/key`).
        /// Most S3-compatible servers require this.
        pub path_style: bool,
        /// Explicit access key. If None, uses env/profile credentials.
        pub access_key: Option<&'a str>,
        /// Explicit secret key. If None, uses env/profile credentials.
        pub secret_key: Option<&'a str>,
    }

    impl S3StorageProvider {
        /// Create for standard AWS S3.
        pub async fn new(bucket: &str, region: Option<&str>, name: &str) -> anyhow::Result<Self> {
            Self::with_options(S3Options {
                bucket,
                region,
                name,
                endpoint_url: None,
                path_style: false,
                access_key: None,
                secret_key: None,
            })
            .await
        }

        /// Create for an S3-compatible service (MinIO, RustFS, Garage, etc.)
        pub async fn s3_compatible(
            bucket: &str,
            endpoint_url: &str,
            region: Option<&str>,
            name: &str,
            access_key: Option<&str>,
            secret_key: Option<&str>,
        ) -> anyhow::Result<Self> {
            Self::with_options(S3Options {
                bucket,
                region: Some(region.unwrap_or("us-east-1")),
                name,
                endpoint_url: Some(endpoint_url),
                path_style: true,
                access_key,
                secret_key,
            })
            .await
        }

        /// Create with full options.
        pub async fn with_options(opts: S3Options<'_>) -> anyhow::Result<Self> {
            let mut config_loader = aws_config::from_env();

            if let Some(r) = opts.region {
                config_loader = config_loader.region(aws_config::Region::new(r.to_string()));
            }

            // If explicit credentials are provided, inject them
            if let (Some(ak), Some(sk)) = (opts.access_key, opts.secret_key) {
                let creds =
                    aws_sdk_s3::config::Credentials::new(ak, sk, None, None, "enigma-config");
                config_loader = config_loader.credentials_provider(creds);
            }

            let sdk_config = config_loader.load().await;

            let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&sdk_config);

            if let Some(endpoint) = opts.endpoint_url {
                s3_config_builder = s3_config_builder.endpoint_url(endpoint);
            }

            if opts.path_style {
                s3_config_builder = s3_config_builder.force_path_style(true);
            }

            let client = Client::from_conf(s3_config_builder.build());

            Ok(Self {
                client,
                bucket: opts.bucket.to_string(),
                name: opts.name.to_string(),
            })
        }
    }

    #[async_trait]
    impl StorageProvider for S3StorageProvider {
        async fn upload_chunk(&self, key: &str, data: &[u8]) -> anyhow::Result<()> {
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(key)
                .body(ByteStream::from(data.to_vec()))
                .send()
                .await?;
            Ok(())
        }

        async fn download_chunk(&self, key: &str) -> anyhow::Result<Vec<u8>> {
            let resp = self
                .client
                .get_object()
                .bucket(&self.bucket)
                .key(key)
                .send()
                .await?;
            let data = resp.body.collect().await?;
            Ok(data.to_vec())
        }

        async fn delete_chunk(&self, key: &str) -> anyhow::Result<()> {
            self.client
                .delete_object()
                .bucket(&self.bucket)
                .key(key)
                .send()
                .await?;
            Ok(())
        }

        async fn chunk_exists(&self, key: &str) -> anyhow::Result<bool> {
            match self
                .client
                .head_object()
                .bucket(&self.bucket)
                .key(key)
                .send()
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
            self.client
                .head_bucket()
                .bucket(&self.bucket)
                .send()
                .await?;
            Ok(())
        }

        fn name(&self) -> &str {
            &self.name
        }
    }
}

#[cfg(feature = "s3")]
pub use inner::{S3Options, S3StorageProvider};
