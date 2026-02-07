use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;

use enigma_core::config::EnigmaConfig;
use enigma_core::crypto::decrypt_chunk;
use enigma_core::dedup::compute_hash;
use enigma_core::manifest::ManifestDb;
use enigma_core::types::{ChunkHash, EncryptedChunk, KeyMaterial};
use enigma_keys::local::LocalKeyProvider;
use enigma_keys::provider::KeyProvider;

use super::providers::init_providers;

pub async fn run(
    backup_id: &str,
    dest: &Path,
    base_dir: &Path,
    cli_passphrase: &Option<String>,
) -> Result<()> {
    println!("Restoring backup {backup_id} to {}", dest.display());

    // Load config
    let config_path = EnigmaConfig::default_path(base_dir);
    let config = EnigmaConfig::load(&config_path)?;

    // Open database
    let db = ManifestDb::open(Path::new(&config.enigma.db_path))?;

    // Verify backup exists
    let backup = db.get_backup(backup_id)?;
    println!(
        "Backup: {} files, {} bytes, created {}",
        backup.total_files, backup.total_bytes, backup.created_at
    );

    // Get key provider
    let passphrase = crate::get_passphrase(cli_passphrase)?;
    let keyfile_path = Path::new(&config.enigma.keyfile_path);
    let key_provider = LocalKeyProvider::open(keyfile_path, passphrase.as_bytes())?;

    // Initialize storage providers
    let storage_providers = init_providers(&config.providers, &db).await?;

    // Create destination directory
    std::fs::create_dir_all(dest)?;

    // Get files in this backup
    let files = db.list_backup_files(backup_id)?;
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=>-"),
    );

    for (file_id, file_path, _file_size, file_hash) in &files {
        pb.set_message(file_path.clone());

        let dest_file = dest.join(file_path);
        if let Some(parent) = dest_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Get ordered chunks for this file
        let file_chunks = db.get_file_chunks(*file_id)?;

        let mut file_data = Vec::new();
        for (chunk_hash, _chunk_index, _offset) in &file_chunks {
            // Get chunk info from DB
            let chunk_info = db
                .get_chunk_info(chunk_hash)?
                .ok_or_else(|| anyhow::anyhow!("Chunk {chunk_hash} not found in database"))?;
            let (nonce, key_id, provider_id, storage_key, _size, size_compressed) = chunk_info;

            // Download from storage
            let provider = storage_providers
                .get(&provider_id)
                .ok_or_else(|| anyhow::anyhow!("Provider {provider_id} not found"))?;
            let ciphertext = provider.download_chunk(&storage_key).await?;

            // Get the key
            let managed_key = key_provider.get_key_by_id(&key_id).await?;
            let key_material = KeyMaterial {
                id: managed_key.id.clone(),
                key: managed_key.key,
            };

            // Decrypt
            let nonce_arr: [u8; 12] = nonce
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid nonce length"))?;
            let hash_bytes: [u8; 32] = hex_decode(chunk_hash)?;
            let encrypted = EncryptedChunk {
                hash: ChunkHash(hash_bytes),
                nonce: nonce_arr,
                ciphertext,
                key_id: key_material.id.clone(),
            };

            let decrypted = decrypt_chunk(&encrypted, &key_material)?;

            // Decompress if this chunk was compressed
            let plaintext = if size_compressed.is_some() {
                enigma_core::compression::decompress_chunk(&decrypted)?
            } else {
                decrypted
            };

            // Verify chunk hash
            let computed = compute_hash(&plaintext);
            if computed.to_hex() != *chunk_hash {
                anyhow::bail!(
                    "Hash mismatch for chunk {chunk_hash}: got {}",
                    computed.to_hex()
                );
            }

            file_data.extend_from_slice(&plaintext);
        }

        // Write file
        std::fs::write(&dest_file, &file_data)?;

        // Verify file hash
        let restored_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&file_data);
            format!("{:x}", hasher.finalize())
        };

        if restored_hash != *file_hash {
            anyhow::bail!(
                "File hash mismatch for {file_path}: expected {file_hash}, got {restored_hash}"
            );
        }

        pb.inc(1);
    }

    pb.finish_with_message("done");
    println!("\nRestore completed: {} files", files.len());

    Ok(())
}

fn hex_decode(hex: &str) -> Result<[u8; 32]> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| anyhow::anyhow!("{e}")))
        .collect::<Result<Vec<_>>>()?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid hash length"))
}
