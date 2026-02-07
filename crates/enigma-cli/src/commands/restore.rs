use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;

use enigma_core::config::EnigmaConfig;
use enigma_core::crypto::decrypt_chunk;
use enigma_core::dedup::compute_hash;
use enigma_core::manifest::ManifestDb;
use enigma_core::types::{ChunkHash, EncryptedChunk, KeyMaterial};

use super::providers::init_providers;

pub async fn run(
    backup_id: &str,
    dest: &Path,
    base_dir: &Path,
    cli_passphrase: &Option<String>,
    path_filter: Option<&str>,
    glob_filter: Option<&str>,
    list_only: bool,
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

    // Get key provider via factory
    let passphrase = if config.enigma.key_provider == "local" {
        Some(crate::get_passphrase(cli_passphrase)?)
    } else {
        None
    };
    let key_provider = enigma_keys::factory::create_key_provider(
        &config.enigma.key_provider,
        passphrase.as_deref().map(|s| s.as_bytes()),
        &config.enigma.keyfile_path,
        config.enigma.vault_url.as_deref(),
        config.enigma.gcp_project_id.as_deref(),
        config.enigma.aws_region.as_deref(),
        config.enigma.secret_prefix.as_deref(),
    )
    .await?;

    // Initialize storage providers
    let storage_providers = init_providers(&config.providers, &db).await?;

    // Create destination directory
    std::fs::create_dir_all(dest)?;

    // Get files in this backup
    let all_files = db.list_backup_files(backup_id)?;

    // Apply filters
    let glob_pattern = glob_filter.map(|g| glob::Pattern::new(g)).transpose()?;
    let files: Vec<_> = all_files
        .into_iter()
        .filter(|(_id, path, _size, _hash)| {
            if let Some(prefix) = path_filter {
                if !path.starts_with(prefix) {
                    return false;
                }
            }
            if let Some(ref pat) = glob_pattern {
                if !pat.matches(path) {
                    return false;
                }
            }
            true
        })
        .collect();

    if list_only {
        println!("\nFiles in backup ({} matching):", files.len());
        for (_id, path, size, hash) in &files {
            println!("  {path}  ({size} bytes)  {hash}");
        }
        return Ok(());
    }

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
            // Get chunk locations (with replica fallback)
            let (nonce, key_id, locations, _size_enc, size_compressed) = db
                .get_chunk_locations(chunk_hash)?
                .ok_or_else(|| anyhow::anyhow!("Chunk {chunk_hash} not found in database"))?;

            // Download with fallback across replicas
            let mut ciphertext = None;
            for (pid, skey) in &locations {
                if let Some(provider) = storage_providers.get(pid) {
                    match provider.download_chunk(skey).await {
                        Ok(data) => {
                            ciphertext = Some(data);
                            break;
                        }
                        Err(e) => {
                            eprintln!("WARN: Provider {pid} failed for chunk {chunk_hash}: {e}, trying next");
                        }
                    }
                }
            }
            let ciphertext = ciphertext
                .ok_or_else(|| anyhow::anyhow!("All providers failed for chunk {chunk_hash}"))?;

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
