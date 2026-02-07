use serde::{Deserialize, Serialize};

/// Operations that pass through Raft (metadata only).
/// Data path (chunk bytes) does NOT go through Raft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaftRequest {
    // Namespace ops
    CreateNamespace {
        name: String,
    },
    DeleteNamespace {
        name: String,
    },

    // Object ops
    InsertObject {
        namespace: String,
        key: String,
        size: u64,
        etag: String,
        content_type: Option<String>,
        chunk_count: u32,
        key_id: String,
    },
    DeleteObject {
        namespace: String,
        key: String,
    },

    // Chunk ops
    InsertOrDedupChunk {
        hash: String,
        nonce: Vec<u8>,
        key_id: String,
        provider_id: i64,
        storage_key: String,
        size_plain: u64,
        size_encrypted: u64,
        size_compressed: Option<u64>,
    },
    DecrementChunkRef {
        hash: String,
    },

    // Object-chunk mapping
    InsertObjectChunk {
        object_id: i64,
        chunk_hash: String,
        chunk_index: u32,
        offset: u64,
    },

    // Provider ops
    InsertProvider {
        name: String,
        provider_type: String,
        bucket: String,
        region: Option<String>,
        weight: u32,
    },

    // Multipart ops
    CreateMultipartUpload {
        upload_id: String,
        namespace_id: i64,
        key: String,
    },
    InsertMultipartPart {
        upload_id: String,
        part_number: i32,
        data: Vec<u8>,
        etag: String,
    },
    AbortMultipartUpload {
        upload_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaftResponse {
    Ok,
    NamespaceId(i64),
    ObjectId(i64),
    ProviderId(i64),
    ChunkInserted {
        is_new: bool,
    },
    ChunkDeleted {
        provider_id: i64,
        storage_key: String,
    },
    Error(String),
}
