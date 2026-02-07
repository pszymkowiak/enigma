use futures::StreamExt;
use s3s::dto::*;
use s3s::s3_error;
use s3s::{S3Response, S3Result};
use sha2::{Digest, Sha256};

use enigma_core::compression::compress_chunk;
use enigma_core::crypto::encrypt_chunk;
use enigma_core::dedup::compute_hash;

use crate::SharedState;

/// Handle PutObject: chunk → encrypt → dedup → distribute → record metadata.
pub async fn handle_put_object(
    state: &SharedState,
    bucket: &str,
    key: &str,
    content_type: Option<String>,
    body: Option<StreamingBlob>,
) -> S3Result<S3Response<PutObjectOutput>> {
    // Read the full body
    let data = read_body(body).await?;
    let total_size = data.len() as u64;

    // Compute overall SHA-256 for the ETag
    let etag = {
        let mut hasher = Sha256::new();
        hasher.update(&data);
        format!("{:x}", hasher.finalize())
    };

    // Chunk the data
    let raw_chunks = chunk_data(&data);
    let chunk_count = raw_chunks.len() as u32;

    // Get namespace
    let ns_id = {
        let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;
        db.get_namespace_id(bucket)
            .map_err(|_| s3_error!(InternalError))?
            .ok_or_else(|| s3_error!(NoSuchBucket))?
    };

    // Process each chunk: encrypt, dedup, upload
    let mut chunk_records = Vec::with_capacity(raw_chunks.len());

    let compression = &state.config.enigma.compression;

    for (idx, chunk_bytes) in raw_chunks.iter().enumerate() {
        let chunk_hash = compute_hash(chunk_bytes);
        let hash_hex = chunk_hash.to_hex();
        let storage_key = chunk_hash.storage_key();

        // Compress (optional, before encryption)
        let (data_to_encrypt, size_compressed) = if compression.enabled {
            let compressed = compress_chunk(chunk_bytes, compression.level)
                .map_err(|_| s3_error!(InternalError))?;
            let sz = compressed.len() as u64;
            (compressed, Some(sz))
        } else {
            (chunk_bytes.to_vec(), None)
        };

        // Encrypt
        let encrypted = encrypt_chunk(&data_to_encrypt, &chunk_hash, &state.key_material)
            .map_err(|_| s3_error!(InternalError))?;

        // Pick provider
        let target_provider = state.distributor.next_provider();

        // Dedup check + insert in DB
        let is_new = {
            let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;
            db.insert_or_dedup_chunk(
                &hash_hex,
                &encrypted.nonce,
                &state.key_material.id,
                target_provider.id,
                &storage_key,
                chunk_bytes.len() as u64,
                encrypted.ciphertext.len() as u64,
                size_compressed,
            )
            .map_err(|_| s3_error!(InternalError))?
        };

        if is_new {
            if let Some(provider) = state.providers.get(&target_provider.id) {
                provider
                    .upload_chunk(&storage_key, &encrypted.ciphertext)
                    .await
                    .map_err(|_| s3_error!(InternalError))?;
            }
        }

        chunk_records.push((hash_hex, idx as u32, chunk_bytes.len() as u64));
    }

    // Insert object record + chunk mappings
    {
        let mut offset = 0u64;
        let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;

        let object_id = db
            .insert_object(
                ns_id,
                key,
                total_size,
                &etag,
                content_type.as_deref(),
                chunk_count,
                &state.key_material.id,
            )
            .map_err(|_| s3_error!(InternalError))?;

        for (hash_hex, chunk_index, size) in &chunk_records {
            db.insert_object_chunk(object_id, hash_hex, *chunk_index, offset)
                .map_err(|_| s3_error!(InternalError))?;
            offset += size;
        }
    }

    let mut output = PutObjectOutput::default();
    output.e_tag = Some(format!("\"{etag}\""));
    Ok(S3Response::new(output))
}

/// Read the full body from a StreamingBlob into a Vec<u8>.
pub async fn read_body(body: Option<StreamingBlob>) -> S3Result<Vec<u8>> {
    let Some(mut body) = body else {
        return Ok(vec![]);
    };
    let mut data = Vec::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| s3_error!(InternalError))?;
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}

/// Chunk data and return owned Vec<Vec<u8>> — used by multipart completion.
pub fn chunk_data_owned(data: &[u8]) -> Vec<Vec<u8>> {
    chunk_data(data).into_iter().map(|s| s.to_vec()).collect()
}

/// Simple chunking of in-memory data.
fn chunk_data(data: &[u8]) -> Vec<&[u8]> {
    if data.is_empty() {
        return vec![];
    }

    let target = 4 * 1024 * 1024; // 4MB
    let max_size = target * 4; // 16MB

    if data.len() <= max_size {
        return vec![data];
    }

    let min_size = target / 4; // 1MB
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

/// Simple hash-based boundary finder for in-memory CDC.
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
