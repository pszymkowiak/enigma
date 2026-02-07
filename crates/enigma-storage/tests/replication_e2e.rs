/// End-to-end multi-cloud test: Azure Storage + Google Cloud Storage
///
/// No replication — round-robin distribution: chunks alternate between Azure and GCS.
/// Tests the full pipeline: chunk → encrypt → distribute → download → decrypt → verify.
///
/// Env vars required:
///   AZURE_STORAGE_ACCOUNT   (e.g. enigmatest42)
///   AZURE_STORAGE_KEY       (storage account access key)
///   GCS_TEST_BUCKET         (e.g. enigma-test-pszymkowiak)
///
/// Optional:
///   E2E_TEST_SIZE_MB  (default: 32 — set to 4096 for 4 GB test)
///
/// Run:
///   AZURE_STORAGE_ACCOUNT=enigmatest42 AZURE_STORAGE_KEY="..." \
///   GCS_TEST_BUCKET=enigma-test-pszymkowiak \
///   cargo test -p enigma-storage --test replication_e2e -- --nocapture
///
/// Large file test (4 GB):
///   E2E_TEST_SIZE_MB=4096 \
///   AZURE_STORAGE_ACCOUNT=... AZURE_STORAGE_KEY="..." \
///   GCS_TEST_BUCKET=... \
///   cargo test -p enigma-storage --test replication_e2e -- --nocapture
use enigma_core::crypto::{decrypt_chunk, encrypt_chunk};
use enigma_core::dedup::compute_hash;
use enigma_core::distributor::Distributor;
use enigma_core::manifest::ManifestDb;
use enigma_core::types::{ChunkHash, EncryptedChunk, KeyMaterial, ProviderType};
use enigma_storage::provider::StorageProvider;
use std::collections::HashMap;
use std::time::Instant;

fn get_test_size_bytes() -> usize {
    let mb: usize = std::env::var("E2E_TEST_SIZE_MB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(32);
    mb * 1024 * 1024
}

fn make_key_material() -> KeyMaterial {
    KeyMaterial {
        id: "test-key-1".to_string(),
        key: [0x42; 32],
    }
}

/// Generate pseudo-random data (deterministic, fast)
fn generate_data(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    let mut state: u64 = 0xdeadbeefcafe1234;
    while data.len() < size {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        data.extend_from_slice(&state.to_le_bytes());
    }
    data.truncate(size);
    data
}

/// Simple CDC-like chunking (same logic as enigma-s3/put.rs)
fn chunk_data(data: &[u8]) -> Vec<&[u8]> {
    if data.is_empty() {
        return vec![];
    }
    let target = 4 * 1024 * 1024;
    let max_size = target * 4;

    if data.len() <= max_size {
        return vec![data];
    }

    let min_size = target / 4;
    let mut chunks = Vec::new();
    let mut offset = 0;

    while offset < data.len() {
        let remaining = data.len() - offset;
        let chunk_size = if remaining <= max_size {
            remaining
        } else {
            find_boundary(&data[offset..], min_size, target, max_size)
        };
        chunks.push(&data[offset..offset + chunk_size]);
        offset += chunk_size;
    }
    chunks
}

fn find_boundary(data: &[u8], min_size: usize, target: usize, max_size: usize) -> usize {
    let len = data.len().min(max_size);
    if len <= min_size {
        return len;
    }
    let mask = (1u64 << 22) - 1;
    let mut hash: u64 = 0;
    for i in min_size..len {
        hash = hash.wrapping_mul(31).wrapping_add(data[i] as u64);
        if hash & mask == 0 {
            return i + 1;
        }
        if i >= target && (hash & (mask >> 1)) == 0 {
            return i + 1;
        }
    }
    len
}

fn hex_decode(hex: &str) -> [u8; 32] {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    bytes.try_into().unwrap()
}

#[tokio::test]
async fn multicloud_azure_gcs_e2e() {
    // ── Setup providers ──────────────────────────────────────
    let azure_account = match std::env::var("AZURE_STORAGE_ACCOUNT") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("SKIP: AZURE_STORAGE_ACCOUNT not set");
            return;
        }
    };
    let azure_key = match std::env::var("AZURE_STORAGE_KEY") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("SKIP: AZURE_STORAGE_KEY not set");
            return;
        }
    };
    let gcs_bucket = match std::env::var("GCS_TEST_BUCKET") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("SKIP: GCS_TEST_BUCKET not set");
            return;
        }
    };

    let azure = enigma_storage::azure::AzureStorageProvider::new(
        &azure_account,
        &azure_key,
        "enigma-chunks",
        "azure-e2e",
    )
    .expect("Azure init failed");
    azure
        .test_connection()
        .await
        .expect("Azure connection failed");

    let gcs = enigma_storage::gcs::GcsStorageProvider::new(&gcs_bucket, "gcs-e2e")
        .await
        .expect("GCS init failed");
    gcs.test_connection()
        .await
        .expect("GCS connection failed");

    println!("OK: Both providers connected (Azure + GCS)");

    // ── Setup DB + distributor (round-robin, no replication) ─
    let db = ManifestDb::open_in_memory().unwrap();
    let pid_azure = db
        .insert_provider("azure", ProviderType::Azure, "enigma-chunks", None, 1)
        .unwrap();
    let pid_gcs = db
        .insert_provider("gcs", ProviderType::Gcs, &gcs_bucket, None, 1)
        .unwrap();

    let providers: HashMap<i64, Box<dyn StorageProvider>> = HashMap::from([
        (pid_azure, Box::new(azure) as Box<dyn StorageProvider>),
        (pid_gcs, Box::new(gcs) as Box<dyn StorageProvider>),
    ]);

    let provider_infos = db.list_providers().unwrap();
    let distributor = Distributor::round_robin(provider_infos);

    let key_material = make_key_material();

    // ── Generate test data ───────────────────────────────────
    let test_size = get_test_size_bytes();
    println!(
        "\n=== Multi-Cloud E2E: {} MB — Azure + GCS (round-robin, no replica) ===\n",
        test_size / (1024 * 1024)
    );

    let t_gen = Instant::now();
    let data = generate_data(test_size);
    println!(
        "Generated {} MB in {:?}",
        test_size / (1024 * 1024),
        t_gen.elapsed()
    );

    let file_hash = compute_hash(&data).to_hex();

    // ── PHASE 1: Chunk + Encrypt + Upload (round-robin) ─────
    println!("\n--- Phase 1: Upload (round-robin across Azure + GCS) ---");
    let raw_chunks = chunk_data(&data);
    let chunk_count = raw_chunks.len();
    println!("Chunked into {} chunks (CDC ~4 MB)", chunk_count);

    let t_upload = Instant::now();
    let mut chunks_on_azure = 0u32;
    let mut chunks_on_gcs = 0u32;
    let mut uploaded_keys: Vec<(i64, String)> = Vec::new();

    for (idx, chunk_bytes) in raw_chunks.iter().enumerate() {
        let chunk_hash = compute_hash(chunk_bytes);
        let hash_hex = chunk_hash.to_hex();
        let storage_key = chunk_hash.storage_key();

        let encrypted =
            encrypt_chunk(chunk_bytes, &chunk_hash, &key_material).expect("encryption failed");

        // Round-robin: pick ONE provider
        let target = distributor.next_provider();

        let is_new = db
            .insert_or_dedup_chunk(
                &hash_hex,
                &encrypted.nonce,
                &key_material.id,
                target.id,
                &storage_key,
                chunk_bytes.len() as u64,
                encrypted.ciphertext.len() as u64,
                None,
            )
            .unwrap();

        if is_new {
            let provider = providers.get(&target.id).unwrap();
            provider
                .upload_chunk(&storage_key, &encrypted.ciphertext)
                .await
                .unwrap_or_else(|e| {
                    panic!("Upload failed to {} for chunk {}: {}", target.name, hash_hex, e)
                });

            uploaded_keys.push((target.id, storage_key.clone()));

            if target.id == pid_azure {
                chunks_on_azure += 1;
            } else {
                chunks_on_gcs += 1;
            }
        }

        if (idx + 1) % 10 == 0 || idx == chunk_count - 1 {
            print!("\r  Uploaded {}/{} chunks", idx + 1, chunk_count);
        }
    }

    let upload_elapsed = t_upload.elapsed();
    let upload_mbps = (test_size as f64 / 1_048_576.0) / upload_elapsed.as_secs_f64();
    println!(
        "\n  Upload complete: {:?} ({:.1} MB/s)",
        upload_elapsed, upload_mbps
    );
    println!("  Distribution: Azure={chunks_on_azure}, GCS={chunks_on_gcs}");

    assert!(chunks_on_azure > 0, "no chunks went to Azure");
    assert!(chunks_on_gcs > 0, "no chunks went to GCS");

    // ── PHASE 2: Download + Decrypt + Verify ────────────────
    println!("\n--- Phase 2: Download + decrypt + verify ---");
    let t_download = Instant::now();
    let mut restored_data = Vec::with_capacity(test_size);

    let raw_chunks_2 = chunk_data(&data);
    for (idx, original_chunk) in raw_chunks_2.iter().enumerate() {
        let chunk_hash = compute_hash(original_chunk);
        let hash_hex = chunk_hash.to_hex();

        // get_chunk_info returns (nonce, key_id, provider_id, storage_key, size_enc, size_comp)
        let (nonce, _key_id, provider_id, storage_key, _size_enc, _size_comp) = db
            .get_chunk_info(&hash_hex)
            .unwrap()
            .unwrap_or_else(|| panic!("chunk {hash_hex} not in DB"));

        let provider = providers
            .get(&provider_id)
            .unwrap_or_else(|| panic!("provider {provider_id} not found"));
        let ciphertext = provider
            .download_chunk(&storage_key)
            .await
            .unwrap_or_else(|e| panic!("download failed from provider {provider_id}: {e}"));

        let nonce_arr: [u8; 12] = nonce.try_into().unwrap();
        let encrypted = EncryptedChunk {
            hash: ChunkHash(hex_decode(&hash_hex)),
            nonce: nonce_arr,
            ciphertext,
            key_id: key_material.id.clone(),
        };
        let plaintext = decrypt_chunk(&encrypted, &key_material).expect("decrypt failed");

        let computed = compute_hash(&plaintext);
        assert_eq!(computed.to_hex(), hash_hex, "chunk hash mismatch at {}", idx);

        restored_data.extend_from_slice(&plaintext);

        if (idx + 1) % 10 == 0 || idx == raw_chunks_2.len() - 1 {
            print!("\r  Downloaded {}/{} chunks", idx + 1, raw_chunks_2.len());
        }
    }

    let download_elapsed = t_download.elapsed();
    let download_mbps = (test_size as f64 / 1_048_576.0) / download_elapsed.as_secs_f64();
    println!(
        "\n  Download complete: {:?} ({:.1} MB/s)",
        download_elapsed, download_mbps
    );

    let restored_hash = compute_hash(&restored_data).to_hex();
    assert_eq!(restored_hash, file_hash, "file hash mismatch!");
    println!("  Integrity: PASSED (SHA-256 match)");

    // ── PHASE 3: Cleanup ────────────────────────────────────
    println!("\n--- Phase 3: Cleanup ---");
    let mut cleaned = 0u32;
    for (pid, key) in &uploaded_keys {
        if let Some(provider) = providers.get(pid) {
            let _ = provider.delete_chunk(key).await;
            cleaned += 1;
        }
    }
    println!("  Cleaned up {} chunks", cleaned);

    // ── Summary ─────────────────────────────────────────────
    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║       MULTI-CLOUD E2E TEST — ALL PASSED             ║");
    println!("╠══════════════════════════════════════════════════════╣");
    println!(
        "║  File size:     {:>6} MB                            ║",
        test_size / (1024 * 1024)
    );
    println!(
        "║  Chunks:        {:>6} (CDC ~4 MB)                   ║",
        chunk_count
    );
    println!(
        "║  Azure:         {:>6} chunks                        ║",
        chunks_on_azure
    );
    println!(
        "║  GCS:           {:>6} chunks                        ║",
        chunks_on_gcs
    );
    println!(
        "║  Upload:        {:>6.1} MB/s                          ║",
        upload_mbps
    );
    println!(
        "║  Download:      {:>6.1} MB/s                          ║",
        download_mbps
    );
    println!("║  Integrity:     PASSED (SHA-256)                     ║");
    println!("╚══════════════════════════════════════════════════════╝");
}
