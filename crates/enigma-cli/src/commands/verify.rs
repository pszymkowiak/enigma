use anyhow::Result;
use std::path::Path;

use enigma_core::config::EnigmaConfig;
use enigma_core::crypto::decrypt_chunk;
use enigma_core::dedup::compute_hash;
use enigma_core::manifest::ManifestDb;
use enigma_core::types::{ChunkHash, EncryptedChunk, KeyMaterial};
use enigma_keys::local::LocalKeyProvider;
use enigma_keys::provider::KeyProvider;

use super::providers::init_providers;

pub async fn run(backup_id: &str, base_dir: &Path, cli_passphrase: &Option<String>) -> Result<()> {
    println!("Verifying backup {backup_id}...");

    let config_path = EnigmaConfig::default_path(base_dir);
    let config = EnigmaConfig::load(&config_path)?;
    let db = ManifestDb::open(Path::new(&config.enigma.db_path))?;

    let _backup = db.get_backup(backup_id)?;

    let passphrase = crate::get_passphrase(cli_passphrase)?;
    let keyfile_path = Path::new(&config.enigma.keyfile_path);
    let key_provider = LocalKeyProvider::open(keyfile_path, passphrase.as_bytes())?;

    // Storage providers
    let storage_providers = init_providers(&config.providers, &db).await?;

    let files = db.list_backup_files(backup_id)?;
    let mut errors = 0u32;
    let mut verified = 0u32;

    for (file_id, file_path, _size, _file_hash) in &files {
        let chunks = db.get_file_chunks(*file_id)?;

        for (chunk_hash, _idx, _offset) in &chunks {
            let chunk_info = match db.get_chunk_info(chunk_hash)? {
                Some(info) => info,
                None => {
                    eprintln!("ERROR: chunk {chunk_hash} not found in DB");
                    errors += 1;
                    continue;
                }
            };
            let (nonce, key_id, provider_id, storage_key, _size, size_compressed) = chunk_info;

            // Download
            let provider = match storage_providers.get(&provider_id) {
                Some(p) => p,
                None => {
                    eprintln!("ERROR: provider {provider_id} not available");
                    errors += 1;
                    continue;
                }
            };

            let ciphertext = match provider.download_chunk(&storage_key).await {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("ERROR: chunk {chunk_hash} download failed: {e}");
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
