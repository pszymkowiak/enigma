/// Integration tests for Azure Blob Storage and Google Cloud Storage providers.
///
/// These tests require real cloud credentials and are skipped if env vars are not set.
///
/// Run with:
///   AZURE_STORAGE_ACCOUNT=enigmatest42 \
///   AZURE_STORAGE_KEY="..." \
///   GCS_TEST_BUCKET=enigma-test-pszymkowiak \
///   cargo test -p enigma-storage --test cloud_providers -- --nocapture
use enigma_storage::provider::StorageProvider;

#[cfg(feature = "azure")]
mod azure_tests {
    use super::*;
    use enigma_storage::azure::AzureStorageProvider;

    fn get_azure_provider() -> Option<AzureStorageProvider> {
        let account = std::env::var("AZURE_STORAGE_ACCOUNT").ok()?;
        let key = std::env::var("AZURE_STORAGE_KEY").ok()?;
        AzureStorageProvider::new(&account, &key, "enigma-chunks", "azure-test").ok()
    }

    #[tokio::test]
    async fn azure_connection() {
        let Some(provider) = get_azure_provider() else {
            eprintln!("SKIP: AZURE_STORAGE_ACCOUNT not set");
            return;
        };
        provider
            .test_connection()
            .await
            .expect("Azure connection failed");
        println!("OK: Azure connection succeeded");
    }

    #[tokio::test]
    async fn azure_upload_download_delete() {
        let Some(provider) = get_azure_provider() else {
            eprintln!("SKIP: AZURE_STORAGE_ACCOUNT not set");
            return;
        };

        let key = "enigma/test/integration-test-chunk";
        let data = b"Hello from Enigma integration test - Azure!";

        // Upload
        provider
            .upload_chunk(key, data)
            .await
            .expect("upload failed");
        println!("OK: Azure upload");

        // Exists
        assert!(provider.chunk_exists(key).await.expect("exists failed"));
        println!("OK: Azure chunk exists");

        // Download
        let downloaded = provider.download_chunk(key).await.expect("download failed");
        assert_eq!(downloaded, data);
        println!("OK: Azure download matches");

        // Delete
        provider.delete_chunk(key).await.expect("delete failed");
        println!("OK: Azure delete");

        // Verify deleted
        assert!(!provider.chunk_exists(key).await.expect("exists failed"));
        println!("OK: Azure chunk deleted");
    }
}

#[cfg(feature = "gcs")]
mod gcs_tests {
    use super::*;
    use enigma_storage::gcs::GcsStorageProvider;

    async fn get_gcs_provider() -> Option<GcsStorageProvider> {
        let bucket = std::env::var("GCS_TEST_BUCKET").ok()?;
        if bucket.is_empty() {
            return None;
        }
        GcsStorageProvider::new(&bucket, "gcs-test").await.ok()
    }

    #[tokio::test]
    async fn gcs_connection() {
        let Some(provider) = get_gcs_provider().await else {
            eprintln!("SKIP: GCS_TEST_BUCKET not set");
            return;
        };
        provider
            .test_connection()
            .await
            .expect("GCS connection failed");
        println!("OK: GCS connection succeeded");
    }

    #[tokio::test]
    async fn gcs_upload_download_delete() {
        let Some(provider) = get_gcs_provider().await else {
            eprintln!("SKIP: GCS_TEST_BUCKET not set");
            return;
        };

        let key = "enigma/test/integration-test-chunk";
        let data = b"Hello from Enigma integration test - GCS!";

        // Upload
        provider
            .upload_chunk(key, data)
            .await
            .expect("upload failed");
        println!("OK: GCS upload");

        // Exists
        assert!(provider.chunk_exists(key).await.expect("exists failed"));
        println!("OK: GCS chunk exists");

        // Download
        let downloaded = provider.download_chunk(key).await.expect("download failed");
        assert_eq!(downloaded, data);
        println!("OK: GCS download matches");

        // Delete
        provider.delete_chunk(key).await.expect("delete failed");
        println!("OK: GCS delete");

        // Verify deleted
        assert!(!provider.chunk_exists(key).await.expect("exists failed"));
        println!("OK: GCS chunk deleted");
    }
}
