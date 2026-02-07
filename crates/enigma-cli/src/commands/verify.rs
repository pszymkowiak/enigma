use anyhow::Result;
use std::path::Path;

use super::providers::init_providers;
use enigma_core::config::EnigmaConfig;
use enigma_core::crypto::decrypt_chunk;
use enigma_core::dedup::compute_hash;
use enigma_core::manifest::ManifestDb;
use enigma_core::types::{ChunkHash, EncryptedChunk, KeyMaterial};

pub async fn run(backup_id: &str, base_dir: &Path, cli_passphrase: &Option<String>) -> Result<()> {
    println!("Verifying backup {backup_id}...");

    let config_path = EnigmaConfig::default_path(base_dir);
    let config = EnigmaConfig::load(&config_path)?;
    let db = ManifestDb::open(Path::new(&config.enigma.db_path))?;

    let _backup = db.get_backup(backup_id)?;

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

    // Storage providers
    let storage_providers = init_providers(&config.providers, &db).await?;

    let files = db.list_backup_files(backup_id)?;
    let mut errors = 0u32;
    let mut verified = 0u32;

    for (file_id, file_path, _size, _file_hash) in &files {
        let chunks = db.get_file_chunks(*file_id)?;

        for (chunk_hash, _idx, _offset) in &chunks {
            let chunk_locations = match db.get_chunk_locations(chunk_hash)? {
                Some(info) => info,
                None => {
                    eprintln!("ERROR: chunk {chunk_hash} not found in DB");
                    errors += 1;
                    continue;
                }
            };
            let (nonce, key_id, locations, _size_enc, size_compressed) = chunk_locations;

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
                            eprintln!("WARN: provider {pid} failed for chunk {chunk_hash}: {e}, trying next");
                        }
                    }
                }
            }
            let ciphertext = match ciphertext {
                Some(data) => data,
                None => {
                    eprintln!("ERROR: all providers failed for chunk {chunk_hash}");
                    errors += 1;
                    continue;
                }
            };

            // Decrypt + verify
            let managed_key = key_provider.get_key_by_id(&key_id).await?;
            let key_material = KeyMaterial {
                id: managed_key.id.clone(),
                key: managed_key.key,
            };

            let nonce_arr: [u8; 12] = nonce
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid nonce"))?;

            let hash_bytes: Vec<u8> = (0..chunk_hash.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&chunk_hash[i..i + 2], 16))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let hash_arr: [u8; 32] = hash_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid hash"))?;

            let encrypted = EncryptedChunk {
                hash: ChunkHash(hash_arr),
                nonce: nonce_arr,
                ciphertext,
                key_id: key_material.id.clone(),
            };

            match decrypt_chunk(&encrypted, &key_material) {
                Ok(decrypted) => {
                    let plaintext = if size_compressed.is_some() {
                        match enigma_core::compression::decompress_chunk(&decrypted) {
                            Ok(d) => d,
                            Err(e) => {
                                eprintln!(
                                    "ERROR: decompression failed for chunk {chunk_hash}: {e}"
                                );
                                errors += 1;
                                continue;
                            }
                        }
                    } else {
                        decrypted
                    };
                    let computed = compute_hash(&plaintext);
                    if computed.to_hex() != *chunk_hash {
                        eprintln!(
                            "ERROR: hash mismatch for chunk in {file_path}: expected {chunk_hash}, got {}",
                            computed.to_hex()
                        );
                        errors += 1;
                    } else {
                        verified += 1;
                    }
                }
                Err(e) => {
                    eprintln!("ERROR: decryption failed for chunk {chunk_hash}: {e}");
                    errors += 1;
                }
            }
        }
    }

    if errors == 0 {
        println!("Verification PASSED: {verified} chunks verified, 0 errors");
    } else {
        println!("Verification FAILED: {verified} chunks OK, {errors} errors");
    }

    Ok(())
}
