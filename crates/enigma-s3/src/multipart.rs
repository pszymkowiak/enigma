use md5::{Digest as Md5Digest, Md5};
use s3s::dto::*;
use s3s::s3_error;
use s3s::{S3Response, S3Result};
use sha2::Sha256;

use crate::SharedState;
use crate::put::read_body;

/// Handle CreateMultipartUpload: create a pending upload entry.
pub async fn handle_create_multipart_upload(
    state: &SharedState,
    bucket: &str,
    key: &str,
) -> S3Result<S3Response<CreateMultipartUploadOutput>> {
    let upload_id = uuid::Uuid::now_v7().to_string();

    let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;
    let ns_id = db
        .get_namespace_id(bucket)
        .map_err(|_| s3_error!(InternalError))?
        .ok_or_else(|| s3_error!(NoSuchBucket))?;

    db.create_multipart_upload(&upload_id, ns_id, key)
        .map_err(|_| s3_error!(InternalError))?;

    let mut output = CreateMultipartUploadOutput::default();
    output.bucket = Some(bucket.to_string());
    output.key = Some(key.to_string());
    output.upload_id = Some(upload_id);

    Ok(S3Response::new(output))
}

/// Handle UploadPart: buffer part data in the database.
pub async fn handle_upload_part(
    state: &SharedState,
    upload_id: &str,
    part_number: i32,
    body: Option<StreamingBlob>,
) -> S3Result<S3Response<UploadPartOutput>> {
    let data = read_body(body).await?;

    // Compute MD5 for ETag (S3 convention for parts)
    let etag = {
        let hash = Md5::digest(&data);
        format!("{:x}", hash)
    };

    let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;

    // Verify upload exists
    db.get_multipart_upload(upload_id)
        .map_err(|_| s3_error!(InternalError))?
        .ok_or_else(|| s3_error!(NoSuchUpload))?;

    db.insert_multipart_part(upload_id, part_number, &data, &etag)
        .map_err(|_| s3_error!(InternalError))?;

    let mut output = UploadPartOutput::default();
    output.e_tag = Some(format!("\"{etag}\""));

    Ok(S3Response::new(output))
}

/// Handle CompleteMultipartUpload: assemble parts, chunk, encrypt, upload.
pub async fn handle_complete_multipart_upload(
    state: &SharedState,
    bucket: &str,
    key: &str,
    upload_id: &str,
) -> S3Result<S3Response<CompleteMultipartUploadOutput>> {
    // Get all parts and assemble
    let (ns_id, assembled_data) = {
        let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;
        let ns_id = db
            .get_namespace_id(bucket)
            .map_err(|_| s3_error!(InternalError))?
            .ok_or_else(|| s3_error!(NoSuchBucket))?;

        let parts = db
            .get_multipart_parts(upload_id)
            .map_err(|_| s3_error!(InternalError))?;

        if parts.is_empty() {
            return Err(s3_error!(InvalidPart));
        }

        let mut assembled = Vec::new();
        for (_part_number, data, _size, _etag) in parts {
            assembled.extend_from_slice(&data);
        }

        (ns_id, assembled)
    };

    // Now process like a PutObject: chunk, encrypt, dedup, upload
    let total_size = assembled_data.len() as u64;
    let etag = {
        let mut hasher = Sha256::new();
        hasher.update(&assembled_data);
        format!("{:x}", hasher.finalize())
    };

    // Chunk the assembled data
    let raw_chunks = crate::put::chunk_data_owned(&assembled_data);

    let mut chunk_records = Vec::new();

    let compression = &state.config.enigma.compression;

    for (idx, chunk_data) in raw_chunks.iter().enumerate() {
        let chunk_hash = enigma_core::dedup::compute_hash(chunk_data);
        let hash_hex = chunk_hash.to_hex();
        let storage_key = chunk_hash.storage_key();

        // Compress (optional, before encryption)
        let (data_to_encrypt, size_compressed) = if compression.enabled {
            let compressed =
                enigma_core::compression::compress_chunk(chunk_data, compression.level)
                    .map_err(|_| s3_error!(InternalError))?;
            let sz = compressed.len() as u64;
            (compressed, Some(sz))
        } else {
            (chunk_data.clone(), None)
        };

        let encrypted =
            enigma_core::crypto::encrypt_chunk(&data_to_encrypt, &chunk_hash, &state.key_material)
                .map_err(|_| s3_error!(InternalError))?;

        let target_provider = state.distributor.next_provider();

        let is_new = {
            let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;
            db.insert_or_dedup_chunk(
                &hash_hex,
                &encrypted.nonce,
                &state.key_material.id,
                target_provider.id,
                &storage_key,
                chunk_data.len() as u64,
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

        chunk_records.push((hash_hex, idx as u32, chunk_data.len() as u64));
    }

    // Insert object + cleanup multipart
    {
        let db = state.db.lock().map_err(|_| s3_error!(InternalError))?;

        let object_id = db
            .insert_object(
                ns_id,
                key,
                total_size,
                &etag,
                None,
                chunk_records.len() as u32,
                &state.key_material.id,
            )
            .map_err(|_| s3_error!(InternalError))?;

        let mut offset = 0u64;
        for (hash_hex, chunk_index, size) in &chunk_records {
            db.insert_object_chunk(object_id, hash_hex, *chunk_index, offset)
                .map_err(|_| s3_error!(InternalError))?;
            offset += size;
        }

        // Cleanup multipart
        db.abort_multipart_upload(upload_id)
            .map_err(|_| s3_error!(InternalError))?;
    }

    let mut output = CompleteMultipartUploadOutput::default();
    output.bucket = Some(bucket.to_string());
    output.key = Some(key.to_string());
    output.e_tag = Some(format!("\"{etag}\""));
    output.location = Some(format!("/{bucket}/{key}"));

    Ok(S3Response::new(output))
}
