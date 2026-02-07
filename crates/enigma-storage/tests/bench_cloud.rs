/// Multi-cloud benchmark: 2 Azure Blob Storage + 2 Google Cloud Storage
///
/// Env vars required:
///   AZURE_ACCOUNT_1, AZURE_KEY_1  (westeurope)
///   AZURE_ACCOUNT_2, AZURE_KEY_2  (northeurope)
///   GCS_BUCKET_1                   (europe-west1)
///   GCS_BUCKET_2                   (europe-west4)
///
/// Run:
///   source /tmp/enigma-bench-keys.env
///   AZURE_ACCOUNT_1=enigmabench1 AZURE_KEY_1=$AZ_KEY1 \
///   AZURE_ACCOUNT_2=enigmabench2 AZURE_KEY_2=$AZ_KEY2 \
///   GCS_BUCKET_1=enigma-bench-1 GCS_BUCKET_2=enigma-bench-2 \
///   cargo test -p enigma-storage --test bench_cloud -- --nocapture
use enigma_storage::provider::StorageProvider;
use std::time::Instant;

struct BenchResult {
    provider: String,
    upload_ms: u128,
    download_ms: u128,
    size_bytes: usize,
    upload_mbps: f64,
    download_mbps: f64,
}

async fn bench_provider(provider: &dyn StorageProvider, name: &str, data: &[u8]) -> BenchResult {
    let key = format!("enigma/bench/{}", uuid::Uuid::now_v7());
    let size = data.len();

    // Upload
    let t0 = Instant::now();
    provider
        .upload_chunk(&key, data)
        .await
        .expect("upload failed");
    let upload_ms = t0.elapsed().as_millis();

    // Download
    let t1 = Instant::now();
    let downloaded = provider
        .download_chunk(&key)
        .await
        .expect("download failed");
    let download_ms = t1.elapsed().as_millis();

    assert_eq!(downloaded.len(), size, "size mismatch");

    // Cleanup
    let _ = provider.delete_chunk(&key).await;

    let upload_mbps = if upload_ms > 0 {
        (size as f64 / 1_048_576.0) / (upload_ms as f64 / 1000.0)
    } else {
        0.0
    };
    let download_mbps = if download_ms > 0 {
        (size as f64 / 1_048_576.0) / (download_ms as f64 / 1000.0)
    } else {
        0.0
    };

    BenchResult {
        provider: name.to_string(),
        upload_ms,
        download_ms,
        size_bytes: size,
        upload_mbps,
        download_mbps,
    }
}

fn print_results(results: &[BenchResult]) {
    println!();
    println!("╔═══════════════════════════════╦════════╦══════════════════╦══════════════════╗");
    println!("║ Provider                      ║  Size  ║     Upload       ║    Download      ║");
    println!("╠═══════════════════════════════╬════════╬══════════════════╬══════════════════╣");
    for r in results {
        let size_str = if r.size_bytes >= 1_048_576 {
            format!("{} MB", r.size_bytes / 1_048_576)
        } else {
            format!("{} KB", r.size_bytes / 1024)
        };
        println!(
            "║ {:<29} ║ {:>4}   ║ {:>6} ms {:>5.1} MB/s ║ {:>6} ms {:>5.1} MB/s ║",
            r.provider, size_str, r.upload_ms, r.upload_mbps, r.download_ms, r.download_mbps
        );
    }
    println!("╚═══════════════════════════════╩════════╩══════════════════╩══════════════════╝");
    println!();
}

#[tokio::test]
async fn bench_all_providers() {
    // Build provider list
    let mut providers: Vec<Box<dyn StorageProvider>> = Vec::new();
    let mut names: Vec<String> = Vec::new();

    // Azure 1
    if let (Ok(acc), Ok(key)) = (
        std::env::var("AZURE_ACCOUNT_1"),
        std::env::var("AZURE_KEY_1"),
    ) {
        let p = enigma_storage::azure::AzureStorageProvider::new(
            &acc,
            &key,
            "enigma-chunks",
            "azure-1-westeurope",
        )
        .expect("Azure 1 init failed");
        names.push("Azure (westeurope)".to_string());
        providers.push(Box::new(p));
    }

    // Azure 2
    if let (Ok(acc), Ok(key)) = (
        std::env::var("AZURE_ACCOUNT_2"),
        std::env::var("AZURE_KEY_2"),
    ) {
        let p = enigma_storage::azure::AzureStorageProvider::new(
            &acc,
            &key,
            "enigma-chunks",
            "azure-2-northeurope",
        )
        .expect("Azure 2 init failed");
        names.push("Azure (northeurope)".to_string());
        providers.push(Box::new(p));
    }

    // GCS 1
    if let Ok(bucket) = std::env::var("GCS_BUCKET_1") {
        if !bucket.is_empty() {
            if let Ok(p) =
                enigma_storage::gcs::GcsStorageProvider::new(&bucket, "gcs-1-west1").await
            {
                names.push("GCS (europe-west1)".to_string());
                providers.push(Box::new(p));
            }
        }
    }

    // GCS 2
    if let Ok(bucket) = std::env::var("GCS_BUCKET_2") {
        if !bucket.is_empty() {
            if let Ok(p) =
                enigma_storage::gcs::GcsStorageProvider::new(&bucket, "gcs-2-west4").await
            {
                names.push("GCS (europe-west4)".to_string());
                providers.push(Box::new(p));
            }
        }
    }

    if providers.is_empty() {
        eprintln!("SKIP: No cloud providers configured");
        return;
    }

    println!("\n=== Enigma Multi-Cloud Benchmark ===");
    println!("Providers: {}", names.join(", "));
    println!();

    // Test sizes: 64KB, 256KB, 1MB, 4MB
    let sizes: Vec<(usize, &str)> = vec![
        (64 * 1024, "64 KB"),
        (256 * 1024, "256 KB"),
        (1024 * 1024, "1 MB"),
        (4 * 1024 * 1024, "4 MB"),
    ];

    for (size, label) in &sizes {
        println!("--- Chunk size: {} ---", label);

        // Generate random data
        let data: Vec<u8> = (0..*size).map(|i| (i % 251) as u8).collect();

        let mut results = Vec::new();
        for (i, provider) in providers.iter().enumerate() {
            let r = bench_provider(provider.as_ref(), &names[i], &data).await;
            results.push(r);
        }

        print_results(&results);
    }

    // Summary: average across all sizes
    println!("=== Benchmark Complete ===");
}
