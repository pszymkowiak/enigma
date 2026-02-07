use serde::Serialize;
use sha2::{Digest, Sha256};

use enigma_core::compression::{compress_chunk, decompress_chunk};
use enigma_core::crypto::{decrypt_chunk, encrypt_chunk};
use enigma_core::dedup::compute_hash;
use enigma_core::types::{ChunkHash, EncryptedChunk};

use crate::EnigmaS3State;

// ── Public types ─────────────────────────────────────────────

#[derive(Serialize)]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct FileEntry {
    pub name: String,
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct FolderListing {
    pub path: String,
    pub folders: Vec<FolderEntry>,
    pub files: Vec<FileEntry>,
}

pub struct FileData {
    pub data: Vec<u8>,
    pub size: u64,
    pub etag: String,
    pub content_type: Option<String>,
}

// ── Operations ───────────────────────────────────────────────

/// Store an object (chunk → encrypt → dedup → upload).
pub async fn store_object(
    state: &EnigmaS3State,
    bucket: &str,
    key: &str,
    data: &[u8],
    content_type: Option<&str>,
) -> anyhow::Result<String> {
    let total_size = data.len() as u64;

    let etag = {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    };

    let raw_chunks = crate::put::chunk_data_owned(data);
    let chunk_count = raw_chunks.len() as u32;

    let ns_id = {
        let db = state.db.lock().map_err(|_| anyhow::anyhow!("db lock"))?;
        db.get_namespace_id(bucket)?
            .ok_or_else(|| anyhow::anyhow!("namespace not found: {bucket}"))?
    };

    let compression = &state.config.enigma.compression;
    let mut chunk_records = Vec::with_capacity(raw_chunks.len());

    for (idx, chunk_bytes) in raw_chunks.iter().enumerate() {
        let chunk_hash = compute_hash(chunk_bytes);
        let hash_hex = chunk_hash.to_hex();
        let storage_key = chunk_hash.storage_key();

        let (data_to_encrypt, size_compressed) = if compression.enabled {
            let compressed = compress_chunk(chunk_bytes, compression.level)?;
            let sz = compressed.len() as u64;
            (compressed, Some(sz))
        } else {
            (chunk_bytes.to_vec(), None)
        };

        let encrypted = encrypt_chunk(&data_to_encrypt, &chunk_hash, &state.key_material)?;
        let replication = state.config.enigma.replication_factor.max(1) as usize;
        let targets = state.distributor.next_providers(replication);
        let primary = targets[0];

        let is_new = {
            let db = state.db.lock().map_err(|_| anyhow::anyhow!("db lock"))?;
            db.insert_or_dedup_chunk(
                &hash_hex,
                &encrypted.nonce,
                &state.key_material.id,
                primary.id,
                &storage_key,
                chunk_bytes.len() as u64,
                encrypted.ciphertext.len() as u64,
                size_compressed,
            )?
        };

        if is_new {
            for target in &targets {
                if let Some(provider) = state.providers.get(&target.id) {
                    match provider.upload_chunk(&storage_key, &encrypted.ciphertext).await {
                        Ok(_) => {}
                        Err(e) if target.id == primary.id => return Err(e.into()),
                        Err(e) => {
                            tracing::warn!("Replica upload to provider {} failed: {e}", target.id);
                        }
                    }
                }
            }
            if targets.len() > 1 {
                let replicas: Vec<(i64, &str)> = targets.iter().map(|t| (t.id, storage_key.as_str())).collect();
                let db = state.db.lock().map_err(|_| anyhow::anyhow!("db lock"))?;
                db.insert_chunk_replicas(&hash_hex, &replicas)?;
            }
        }

        chunk_records.push((hash_hex, idx as u32, chunk_bytes.len() as u64));
    }

    {
        let mut offset = 0u64;
        let db = state.db.lock().map_err(|_| anyhow::anyhow!("db lock"))?;
        let object_id = db.insert_object(
            ns_id,
            key,
            total_size,
            &etag,
            content_type,
            chunk_count,
            &state.key_material.id,
        )?;
        for (hash_hex, chunk_index, size) in &chunk_records {
            db.insert_object_chunk(object_id, hash_hex, *chunk_index, offset)?;
            offset += size;
        }
    }

    Ok(etag)
}

/// Retrieve an object (download chunks → decrypt → reassemble).
pub async fn retrieve_object(
    state: &EnigmaS3State,
    bucket: &str,
    key: &str,
) -> anyhow::Result<FileData> {
    let (object_id, size, etag, content_type, _chunk_count, _key_id, _last_modified) = {
        let db = state.db.lock().map_err(|_| anyhow::anyhow!("db lock"))?;
        let ns_id = db
            .get_namespace_id(bucket)?
            .ok_or_else(|| anyhow::anyhow!("namespace not found: {bucket}"))?;
        db.get_object(ns_id, key)?
            .ok_or_else(|| anyhow::anyhow!("object not found: {key}"))?
    };

    let chunk_list = {
        let db = state.db.lock().map_err(|_| anyhow::anyhow!("db lock"))?;
        db.get_object_chunks(object_id)?
    };

    let mut file_data = Vec::with_capacity(size as usize);

    for (chunk_hash_hex, _chunk_index, _offset) in &chunk_list {
        let chunk_locations = {
            let db = state.db.lock().map_err(|_| anyhow::anyhow!("db lock"))?;
            db.get_chunk_locations(chunk_hash_hex)?
                .ok_or_else(|| anyhow::anyhow!("chunk not found: {chunk_hash_hex}"))?
        };
        let (nonce, _chunk_key_id, locations, _size_enc, size_compressed) = chunk_locations;

        // Download with fallback across replicas
        let mut ciphertext = None;
        for (pid, skey) in &locations {
            if let Some(provider) = state.providers.get(pid) {
                match provider.download_chunk(skey).await {
                    Ok(data) => {
                        ciphertext = Some(data);
                        break;
                    }
                    Err(e) => {
                        tracing::warn!("Provider {pid} failed for chunk {chunk_hash_hex}: {e}, trying next");
                    }
                }
            }
        }
        let ciphertext =
            ciphertext.ok_or_else(|| anyhow::anyhow!("all providers failed for chunk {chunk_hash_hex}"))?;

        let nonce_arr: [u8; 12] = nonce
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid nonce length"))?;
        let hash_bytes = hex_decode(chunk_hash_hex)?;

        let encrypted = EncryptedChunk {
            hash: ChunkHash(hash_bytes),
            nonce: nonce_arr,
            ciphertext,
            key_id: state.key_material.id.clone(),
        };

        let decrypted = decrypt_chunk(&encrypted, &state.key_material)?;

        let plaintext = if size_compressed.is_some() {
            decompress_chunk(&decrypted)?
        } else {
            decrypted
        };

        let computed = compute_hash(&plaintext);
        if computed.to_hex() != *chunk_hash_hex {
            anyhow::bail!("chunk hash mismatch for {chunk_hash_hex}");
        }

        file_data.extend_from_slice(&plaintext);
    }

    Ok(FileData {
        data: file_data,
        size,
        etag,
        content_type,
    })
}

/// Remove an object and its orphaned chunks.
pub async fn remove_object(
    state: &EnigmaS3State,
    bucket: &str,
    key: &str,
) -> anyhow::Result<()> {
    let to_delete = {
        let db = state.db.lock().map_err(|_| anyhow::anyhow!("db lock"))?;
        let ns_id = db
            .get_namespace_id(bucket)?
            .ok_or_else(|| anyhow::anyhow!("namespace not found: {bucket}"))?;
        db.delete_object_by_ns_key(ns_id, key)?
    };

    for (provider_id, storage_key) in to_delete {
        if let Some(provider) = state.providers.get(&provider_id) {
            let _ = provider.delete_chunk(&storage_key).await;
        }
    }

    Ok(())
}

/// List folder contents at a given prefix.
pub async fn list_folder(
    state: &EnigmaS3State,
    bucket: &str,
    prefix: &str,
) -> anyhow::Result<FolderListing> {
    let db = state.db.lock().map_err(|_| anyhow::anyhow!("db lock"))?;
    let ns_id = db
        .get_namespace_id(bucket)?
        .ok_or_else(|| anyhow::anyhow!("namespace not found: {bucket}"))?;

    let objects = db.list_objects(ns_id, prefix, 10000, "")?;

    let mut folders = Vec::new();
    let mut files = Vec::new();
    let mut seen_folders = std::collections::BTreeSet::new();

    for (key, size, etag, created_at) in &objects {
        let after_prefix = &key[prefix.len()..];
        if let Some(pos) = after_prefix.find('/') {
            let folder_name = &after_prefix[..pos];
            if seen_folders.insert(folder_name.to_string()) {
                folders.push(FolderEntry {
                    name: folder_name.to_string(),
                    path: format!("{}{}/", prefix, folder_name),
                });
            }
        } else {
            let name = after_prefix.to_string();
            if name.is_empty() {
                continue;
            }
            files.push(FileEntry {
                name,
                key: key.clone(),
                size: *size,
                etag: etag.clone(),
                created_at: created_at.clone(),
            });
        }
    }

    Ok(FolderListing {
        path: prefix.to_string(),
        folders,
        files,
    })
}

/// Ensure a namespace exists (create if missing).
pub fn ensure_namespace(state: &EnigmaS3State, name: &str) -> anyhow::Result<()> {
    let db = state.db.lock().map_err(|_| anyhow::anyhow!("db lock"))?;
    if !db.namespace_exists(name)? {
        db.create_namespace(name)?;
        tracing::info!("Auto-created namespace '{name}'");
    }
    Ok(())
}

fn hex_decode(hex: &str) -> anyhow::Result<[u8; 32]> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| anyhow::anyhow!("{e}")))
        .collect::<anyhow::Result<Vec<_>>>()?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid hash length"))
}
