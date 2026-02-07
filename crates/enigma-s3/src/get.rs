use s3s::dto::*;
use s3s::s3_error;
use s3s::{S3Response, S3Result};

use enigma_core::compression::decompress_chunk;
use enigma_core::crypto::decrypt_chunk;
use enigma_core::dedup::compute_hash;
use enigma_core::types::{ChunkHash, EncryptedChunk};

use crate::SharedState;

/// Handle GetObject: query metadata → download chunks → decrypt → reassemble.
pub async fn handle_get_object(
    state: &SharedState,
    bucket: &str,
    key: &str,
) -> S3Result<S3Response<GetObjectOutput>> {
    // Get object metadata
    let (object_id, size, etag, content_type, _chunk_count, _key_id, _last_modified) = {
        let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;
        let ns_id = db
            .get_namespace_id(bucket)
            .map_err(|_| s3_error!(InternalError))?
            .ok_or_else(|| s3_error!(NoSuchBucket))?;

        db.get_object(ns_id, key)
            .map_err(|_| s3_error!(InternalError))?
            .ok_or_else(|| s3_error!(NoSuchKey))?
    };

    // Get ordered chunks
    let chunk_list = {
        let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;
        db.get_object_chunks(object_id)
            .map_err(|_| s3_error!(InternalError))?
    };

    // Download, decrypt, and reassemble
    let mut file_data = Vec::with_capacity(size as usize);

    for (chunk_hash_hex, _chunk_index, _offset) in &chunk_list {
        let chunk_info = {
            let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;
            db.get_chunk_info(chunk_hash_hex)
                .map_err(|_| s3_error!(InternalError))?
                .ok_or_else(|| s3_error!(InternalError))?
        };
        let (nonce, _chunk_key_id, provider_id, storage_key, _size_enc, size_compressed) =
            chunk_info;

        // Download from storage
        let provider = state
            .providers
            .get(&provider_id)
            .ok_or_else(|| s3_error!(InternalError))?;
        let ciphertext = provider
            .download_chunk(&storage_key)
            .await
            .map_err(|_| s3_error!(InternalError))?;

        // Decrypt
        let nonce_arr: [u8; 12] = nonce.try_into().map_err(|_| s3_error!(InternalError))?;

        let hash_bytes = hex_decode(chunk_hash_hex).map_err(|_| s3_error!(InternalError))?;

        let encrypted = EncryptedChunk {
            hash: ChunkHash(hash_bytes),
            nonce: nonce_arr,
            ciphertext,
            key_id: state.key_material.id.clone(),
        };

        let decrypted =
            decrypt_chunk(&encrypted, &state.key_material).map_err(|_| s3_error!(InternalError))?;

        // Decompress if this chunk was compressed
        let plaintext = if size_compressed.is_some() {
            decompress_chunk(&decrypted).map_err(|_| s3_error!(InternalError))?
        } else {
            decrypted
        };

        // Verify chunk hash
        let computed = compute_hash(&plaintext);
        if computed.to_hex() != *chunk_hash_hex {
            return Err(s3_error!(InternalError));
        }

        file_data.extend_from_slice(&plaintext);
    }

    let mut output = GetObjectOutput::default();
    output.content_length = Some(size as i64);
    output.e_tag = Some(format!("\"{etag}\""));
    output.content_type = content_type.and_then(|ct| ct.parse().ok());
    output.body = Some(StreamingBlob::from(s3s::Body::from(file_data)));

    Ok(S3Response::new(output))
}

fn hex_decode(hex: &str) -> Result<[u8; 32], String> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    bytes
        .try_into()
        .map_err(|_| "Invalid hash length".to_string())
}
