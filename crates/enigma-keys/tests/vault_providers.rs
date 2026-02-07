/// Integration tests for vault-based key providers.
///
/// These tests require real cloud credentials and are skipped if env vars are not set.
///
/// Run with:
///   AZURE_KEYVAULT_URL="https://enigma-keys-test.vault.azure.net/" \
///   cargo test -p enigma-keys --features azure-keyvault --test vault_providers -- --nocapture
///
///   GCP_PROJECT_ID=eastern-rider-263712 \
///   cargo test -p enigma-keys --features gcp-secretmanager --test vault_providers -- --nocapture
use enigma_keys::provider::KeyProvider;

#[cfg(feature = "azure-keyvault")]
mod azure_tests {
    use super::*;
    use enigma_keys::azure_vault::AzureKeyVaultProvider;

    fn get_vault_url() -> Option<String> {
        let url = std::env::var("AZURE_KEYVAULT_URL").ok()?;
        if url.is_empty() {
            return None;
        }
        Some(url)
    }

    #[tokio::test]
    async fn azure_kv_create_and_get_key() {
        let Some(vault_url) = get_vault_url() else {
            eprintln!("SKIP: AZURE_KEYVAULT_URL not set");
            return;
        };

        let mut provider =
            AzureKeyVaultProvider::new(&vault_url, Some("enigma-test")).expect("init failed");

        // Create a key
        let key1 = provider.create_key().await.expect("create_key failed");
        assert_eq!(key1.key.len(), 32);
        println!("OK: Created key {} in Azure KV", key1.id);

        // Get current key
        let current = provider
            .get_current_key()
            .await
            .expect("get_current_key failed");
        assert_eq!(current.id, key1.id);
        assert_eq!(current.key, key1.key);
        println!("OK: Current key matches");

        // Get by ID
        let by_id = provider
            .get_key_by_id(&key1.id)
            .await
            .expect("get_key_by_id failed");
        assert_eq!(by_id.key, key1.key);
        println!("OK: Get by ID matches");

        println!("OK: Azure Key Vault create + get test passed");
    }

    #[tokio::test]
    async fn azure_kv_rotate_key() {
        let Some(vault_url) = get_vault_url() else {
            eprintln!("SKIP: AZURE_KEYVAULT_URL not set");
            return;
        };

        let mut provider =
            AzureKeyVaultProvider::new(&vault_url, Some("enigma-rot")).expect("init failed");

        // Create first key
        let key1 = provider.create_key().await.expect("create_key failed");
        println!("Key 1: {}", key1.id);

        // Rotate
        let key2 = provider.rotate_key().await.expect("rotate_key failed");
        println!("Key 2: {}", key2.id);

        assert_ne!(key1.id, key2.id);
        assert_ne!(key1.key, key2.key);

        // Current should be key2
        let current = provider
            .get_current_key()
            .await
            .expect("get_current failed");
        assert_eq!(current.id, key2.id);

        // Old key still accessible
        let old = provider
            .get_key_by_id(&key1.id)
            .await
            .expect("get old key failed");
        assert_eq!(old.key, key1.key);

        // List should contain both
        let ids = provider.list_key_ids().await.expect("list failed");
        assert!(ids.contains(&key1.id), "key1 not in list");
        assert!(ids.contains(&key2.id), "key2 not in list");
        println!("OK: {} keys listed", ids.len());

        println!("OK: Azure Key Vault rotation test passed");
    }
}

#[cfg(feature = "gcp-secretmanager")]
mod gcp_tests {
    use super::*;
    use enigma_keys::gcp_secretmanager::GcpSecretManagerProvider;

    fn get_project_id() -> Option<String> {
        let pid = std::env::var("GCP_PROJECT_ID").ok()?;
        if pid.is_empty() {
            return None;
        }
        Some(pid)
    }

    #[tokio::test]
    async fn gcp_sm_create_and_get_key() {
        let Some(project_id) = get_project_id() else {
            eprintln!("SKIP: GCP_PROJECT_ID not set");
            return;
        };

        let mut provider = GcpSecretManagerProvider::new(&project_id, Some("enigma-test"))
            .await
            .expect("init failed");

        // Create a key
        let key1 = provider.create_key().await.expect("create_key failed");
        assert_eq!(key1.key.len(), 32);
        println!("OK: Created key {} in GCP SM", key1.id);

        // Get current key
        let current = provider
            .get_current_key()
            .await
            .expect("get_current_key failed");
        assert_eq!(current.id, key1.id);
        assert_eq!(current.key, key1.key);
        println!("OK: Current key matches");

        // Get by ID
        let by_id = provider
            .get_key_by_id(&key1.id)
            .await
            .expect("get_key_by_id failed");
        assert_eq!(by_id.key, key1.key);
        println!("OK: Get by ID matches");

        println!("OK: GCP Secret Manager create + get test passed");
    }

    #[tokio::test]
    async fn gcp_sm_rotate_key() {
        let Some(project_id) = get_project_id() else {
            eprintln!("SKIP: GCP_PROJECT_ID not set");
            return;
        };

        let mut provider = GcpSecretManagerProvider::new(&project_id, Some("enigma-rot"))
            .await
            .expect("init failed");

        // Create first key
        let key1 = provider.create_key().await.expect("create_key failed");
        println!("Key 1: {}", key1.id);

        // Rotate
        let key2 = provider.rotate_key().await.expect("rotate_key failed");
        println!("Key 2: {}", key2.id);

        assert_ne!(key1.id, key2.id);
        assert_ne!(key1.key, key2.key);

        // Current should be key2
        let current = provider
            .get_current_key()
            .await
            .expect("get_current failed");
        assert_eq!(current.id, key2.id);

        // Old key still accessible
        let old = provider
            .get_key_by_id(&key1.id)
            .await
            .expect("get old key failed");
        assert_eq!(old.key, key1.key);

        // List should contain both
        let ids = provider.list_key_ids().await.expect("list failed");
        assert!(ids.contains(&key1.id), "key1 not in list");
        assert!(ids.contains(&key2.id), "key2 not in list");
        println!("OK: {} keys listed", ids.len());

        println!("OK: GCP Secret Manager rotation test passed");
    }
}

#[cfg(feature = "aws-secretsmanager")]
mod aws_tests {
    use super::*;
    use enigma_keys::aws_secretsmanager::AwsSecretManagerProvider;

    fn get_aws_region() -> Option<String> {
        let region = std::env::var("AWS_REGION").ok()?;
        if region.is_empty() {
            return None;
        }
        Some(region)
    }

    #[tokio::test]
    async fn aws_sm_create_and_get_key() {
        let Some(region) = get_aws_region() else {
            eprintln!("SKIP: AWS_REGION not set");
            return;
        };

        let mut provider = AwsSecretManagerProvider::new(&region, Some("enigma-test"))
            .await
            .expect("init failed");

        // Create a key
        let key1 = provider.create_key().await.expect("create_key failed");
        assert_eq!(key1.key.len(), 32);
        println!("OK: Created key {} in AWS SM", key1.id);

        // Get current key
        let current = provider
            .get_current_key()
            .await
            .expect("get_current_key failed");
        assert_eq!(current.id, key1.id);
        assert_eq!(current.key, key1.key);
        println!("OK: Current key matches");

        // Get by ID
        let by_id = provider
            .get_key_by_id(&key1.id)
            .await
            .expect("get_key_by_id failed");
        assert_eq!(by_id.key, key1.key);
        println!("OK: Get by ID matches");

        println!("OK: AWS Secrets Manager create + get test passed");
    }

    #[tokio::test]
    async fn aws_sm_rotate_and_list() {
        let Some(region) = get_aws_region() else {
            eprintln!("SKIP: AWS_REGION not set");
            return;
        };

        let mut provider = AwsSecretManagerProvider::new(&region, Some("enigma-rot"))
            .await
            .expect("init failed");

        // Create first key
        let key1 = provider.create_key().await.expect("create_key failed");
        println!("Key 1: {}", key1.id);

        // Rotate
        let key2 = provider.rotate_key().await.expect("rotate_key failed");
        println!("Key 2: {}", key2.id);

        assert_ne!(key1.id, key2.id);
        assert_ne!(key1.key, key2.key);

        // Current should be key2
        let current = provider
            .get_current_key()
            .await
            .expect("get_current failed");
        assert_eq!(current.id, key2.id);

        // Old key still accessible
        let old = provider
            .get_key_by_id(&key1.id)
            .await
            .expect("get old key failed");
        assert_eq!(old.key, key1.key);

        // List should contain both
        let ids = provider.list_key_ids().await.expect("list failed");
        assert!(ids.contains(&key1.id), "key1 not in list");
        assert!(ids.contains(&key2.id), "key2 not in list");
        println!("OK: {} keys listed", ids.len());

        println!("OK: AWS Secrets Manager rotation test passed");
    }
}
