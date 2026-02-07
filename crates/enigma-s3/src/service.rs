use s3s::dto::*;
use s3s::s3_error;
use s3s::{S3, S3Request, S3Response, S3Result};

use crate::SharedState;

/// The Enigma S3 service implementing the s3s S3 trait.
pub struct EnigmaS3Service {
    pub state: SharedState,
}

impl EnigmaS3Service {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl S3 for EnigmaS3Service {
    // ── Bucket operations ───────────────────────────────────

    async fn create_bucket(
        &self,
        req: S3Request<CreateBucketInput>,
    ) -> S3Result<S3Response<CreateBucketOutput>> {
        let bucket = &req.input.bucket;
        tracing::info!("CreateBucket: {bucket}");

        let db = self.state.db.lock().map_err(|_| s3_error!(InternalError))?;
        if db
            .namespace_exists(bucket)
            .map_err(|_| s3_error!(InternalError))?
        {
            return Err(s3_error!(BucketAlreadyOwnedByYou));
        }
        db.create_namespace(bucket)
            .map_err(|_| s3_error!(InternalError))?;

        let mut output = CreateBucketOutput::default();
        output.location = Some(format!("/{bucket}"));
        Ok(S3Response::new(output))
    }

    async fn delete_bucket(
        &self,
        req: S3Request<DeleteBucketInput>,
    ) -> S3Result<S3Response<DeleteBucketOutput>> {
        let bucket = &req.input.bucket;
        tracing::info!("DeleteBucket: {bucket}");

        let db = self.state.db.lock().map_err(|_| s3_error!(InternalError))?;
        let ns_id = db
            .get_namespace_id(bucket)
            .map_err(|_| s3_error!(InternalError))?
            .ok_or_else(|| s3_error!(NoSuchBucket))?;

        // Check if bucket is empty
        let count = db
            .count_objects_with_prefix(ns_id, "")
            .map_err(|_| s3_error!(InternalError))?;
        if count > 0 {
            return Err(s3_error!(BucketNotEmpty));
        }

        db.delete_namespace(bucket)
            .map_err(|_| s3_error!(InternalError))?;

        Ok(S3Response::new(DeleteBucketOutput::default()))
    }

    async fn head_bucket(
        &self,
        req: S3Request<HeadBucketInput>,
    ) -> S3Result<S3Response<HeadBucketOutput>> {
        let bucket = &req.input.bucket;

        let db = self.state.db.lock().map_err(|_| s3_error!(InternalError))?;
        if !db
            .namespace_exists(bucket)
            .map_err(|_| s3_error!(InternalError))?
        {
            return Err(s3_error!(NoSuchBucket));
        }

        Ok(S3Response::new(HeadBucketOutput::default()))
    }

    async fn list_buckets(
        &self,
        _req: S3Request<ListBucketsInput>,
    ) -> S3Result<S3Response<ListBucketsOutput>> {
        let db = self.state.db.lock().map_err(|_| s3_error!(InternalError))?;
        let namespaces = db.list_namespaces().map_err(|_| s3_error!(InternalError))?;

        let buckets: Vec<Bucket> = namespaces
            .into_iter()
            .map(|(_id, name, _created_at)| Bucket {
                bucket_region: None,
                creation_date: None,
                name: Some(name),
            })
            .collect();

        let mut output = ListBucketsOutput::default();
        output.buckets = Some(buckets);
        output.owner = Some(Owner {
            display_name: Some("enigma".to_string()),
            id: Some("enigma".to_string()),
        });
        Ok(S3Response::new(output))
    }

    // ── Object operations ───────────────────────────────────

    async fn put_object(
        &self,
        req: S3Request<PutObjectInput>,
    ) -> S3Result<S3Response<PutObjectOutput>> {
        let bucket = req.input.bucket.clone();
        let key = req.input.key.clone();
        let content_type = req.input.content_type.map(|m| m.to_string());
        tracing::info!("PutObject: {bucket}/{key}");

        crate::put::handle_put_object(&self.state, &bucket, &key, content_type, req.input.body)
            .await
    }

    async fn get_object(
        &self,
        req: S3Request<GetObjectInput>,
    ) -> S3Result<S3Response<GetObjectOutput>> {
        let bucket = &req.input.bucket;
        let key = &req.input.key;
        tracing::info!("GetObject: {bucket}/{key}");

        crate::get::handle_get_object(&self.state, bucket, key).await
    }

    async fn head_object(
        &self,
        req: S3Request<HeadObjectInput>,
    ) -> S3Result<S3Response<HeadObjectOutput>> {
        let bucket = &req.input.bucket;
        let key = &req.input.key;

        let db = self.state.db.lock().map_err(|_| s3_error!(InternalError))?;
        let ns_id = db
            .get_namespace_id(bucket)
            .map_err(|_| s3_error!(InternalError))?
            .ok_or_else(|| s3_error!(NoSuchBucket))?;

        let obj = db
            .get_object(ns_id, key)
            .map_err(|_| s3_error!(InternalError))?
            .ok_or_else(|| s3_error!(NoSuchKey))?;

        let (_obj_id, size, etag, content_type, _chunk_count, _key_id, _last_modified) = obj;

        let mut output = HeadObjectOutput::default();
        output.content_length = Some(size as i64);
        output.e_tag = Some(format!("\"{etag}\""));
        output.content_type = content_type.and_then(|ct| ct.parse().ok());

        Ok(S3Response::new(output))
    }

    async fn delete_object(
        &self,
        req: S3Request<DeleteObjectInput>,
    ) -> S3Result<S3Response<DeleteObjectOutput>> {
        let bucket = &req.input.bucket;
        let key = &req.input.key;
        tracing::info!("DeleteObject: {bucket}/{key}");

        let to_delete = {
            let db = self.state.db.lock().map_err(|_| s3_error!(InternalError))?;
            let ns_id = db
                .get_namespace_id(bucket)
                .map_err(|_| s3_error!(InternalError))?
                .ok_or_else(|| s3_error!(NoSuchBucket))?;

            db.delete_object_by_ns_key(ns_id, key)
                .map_err(|_| s3_error!(InternalError))?
        };

        // Delete chunks from storage providers
        for (provider_id, storage_key) in to_delete {
            if let Some(provider) = self.state.providers.get(&provider_id) {
                let _ = provider.delete_chunk(&storage_key).await;
            }
        }

        Ok(S3Response::new(DeleteObjectOutput::default()))
    }

    // ── List operations ─────────────────────────────────────

    async fn list_objects_v2(
        &self,
        req: S3Request<ListObjectsV2Input>,
    ) -> S3Result<S3Response<ListObjectsV2Output>> {
        let bucket = &req.input.bucket;
        let prefix = req.input.prefix.as_deref().unwrap_or("");
        let max_keys = req.input.max_keys.unwrap_or(1000);
        let start_after = req.input.start_after.as_deref().unwrap_or("");
        let continuation_token = req.input.continuation_token.as_deref().unwrap_or("");
        let delimiter = req.input.delimiter.as_deref().unwrap_or("");

        tracing::info!("ListObjectsV2: {bucket} prefix={prefix}");

        crate::list::handle_list_objects_v2(
            &self.state,
            bucket,
            prefix,
            delimiter,
            max_keys as u32,
            start_after,
            continuation_token,
        )
        .await
    }

    // ── Multipart operations ────────────────────────────────

    async fn create_multipart_upload(
        &self,
        req: S3Request<CreateMultipartUploadInput>,
    ) -> S3Result<S3Response<CreateMultipartUploadOutput>> {
        let bucket = &req.input.bucket;
        let key = &req.input.key;
        tracing::info!("CreateMultipartUpload: {bucket}/{key}");

        crate::multipart::handle_create_multipart_upload(&self.state, bucket, key).await
    }

    async fn upload_part(
        &self,
        req: S3Request<UploadPartInput>,
    ) -> S3Result<S3Response<UploadPartOutput>> {
        let upload_id = &req.input.upload_id;
        let part_number = req.input.part_number;
        tracing::info!("UploadPart: upload_id={upload_id} part={part_number}");

        crate::multipart::handle_upload_part(&self.state, upload_id, part_number, req.input.body)
            .await
    }

    async fn complete_multipart_upload(
        &self,
        req: S3Request<CompleteMultipartUploadInput>,
    ) -> S3Result<S3Response<CompleteMultipartUploadOutput>> {
        let bucket = &req.input.bucket;
        let key = &req.input.key;
        let upload_id = &req.input.upload_id;
        tracing::info!("CompleteMultipartUpload: {bucket}/{key} upload_id={upload_id}");

        crate::multipart::handle_complete_multipart_upload(&self.state, bucket, key, upload_id)
            .await
    }

    async fn abort_multipart_upload(
        &self,
        req: S3Request<AbortMultipartUploadInput>,
    ) -> S3Result<S3Response<AbortMultipartUploadOutput>> {
        let upload_id = &req.input.upload_id;
        tracing::info!("AbortMultipartUpload: upload_id={upload_id}");

        let db = self.state.db.lock().map_err(|_| s3_error!(InternalError))?;
        db.abort_multipart_upload(upload_id)
            .map_err(|_| s3_error!(InternalError))?;

        Ok(S3Response::new(AbortMultipartUploadOutput::default()))
    }
}
