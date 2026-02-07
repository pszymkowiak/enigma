use std::io::Cursor;
use std::sync::{Arc, Mutex};

use openraft::storage::RaftStateMachine;
use openraft::{
    BasicNode, Entry, EntryPayload, LogId, OptionalSend, Snapshot, SnapshotMeta, StorageError,
    StoredMembership,
};

use enigma_core::manifest::ManifestDb;

use crate::TypeConfig;
use crate::types::{RaftRequest, RaftResponse};

/// Enigma Raft state machine wrapping ManifestDb.
pub struct EnigmaStateMachine {
    pub db: Arc<Mutex<ManifestDb>>,
    last_applied: Mutex<Option<LogId<u64>>>,
    last_membership: Mutex<StoredMembership<u64, BasicNode>>,
}

impl EnigmaStateMachine {
    pub fn new(db: Arc<Mutex<ManifestDb>>) -> Self {
        Self {
            db,
            last_applied: Mutex::new(None),
            last_membership: Mutex::new(StoredMembership::default()),
        }
    }

    /// Apply a single RaftRequest to the ManifestDb.
    fn apply_request(&self, req: &RaftRequest) -> RaftResponse {
        let db = match self.db.lock() {
            Ok(db) => db,
            Err(_) => return RaftResponse::Error("DB lock poisoned".to_string()),
        };

        match req {
            RaftRequest::CreateNamespace { name } => match db.create_namespace(name) {
                Ok(id) => RaftResponse::NamespaceId(id),
                Err(e) => RaftResponse::Error(e.to_string()),
            },
            RaftRequest::DeleteNamespace { name } => match db.delete_namespace(name) {
                Ok(_) => RaftResponse::Ok,
                Err(e) => RaftResponse::Error(e.to_string()),
            },
            RaftRequest::InsertObject {
                namespace,
                key,
                size,
                etag,
                content_type,
                chunk_count,
                key_id,
            } => {
                let ns_id = match db.get_namespace_id(namespace) {
                    Ok(Some(id)) => id,
                    Ok(None) => return RaftResponse::Error("Namespace not found".to_string()),
                    Err(e) => return RaftResponse::Error(e.to_string()),
                };
                match db.insert_object(
                    ns_id,
                    key,
                    *size,
                    etag,
                    content_type.as_deref(),
                    *chunk_count,
                    key_id,
                ) {
                    Ok(id) => RaftResponse::ObjectId(id),
                    Err(e) => RaftResponse::Error(e.to_string()),
                }
            }
            RaftRequest::DeleteObject { namespace, key } => {
                let ns_id = match db.get_namespace_id(namespace) {
                    Ok(Some(id)) => id,
                    Ok(None) => return RaftResponse::Error("Namespace not found".to_string()),
                    Err(e) => return RaftResponse::Error(e.to_string()),
                };
                match db.delete_object_by_ns_key(ns_id, key) {
                    Ok(to_delete) => {
                        if let Some((pid, sk)) = to_delete.into_iter().next() {
                            RaftResponse::ChunkDeleted {
                                provider_id: pid,
                                storage_key: sk,
                            }
                        } else {
                            RaftResponse::Ok
                        }
                    }
                    Err(e) => RaftResponse::Error(e.to_string()),
                }
            }
            RaftRequest::InsertOrDedupChunk {
                hash,
                nonce,
                key_id,
                provider_id,
                storage_key,
                size_plain,
                size_encrypted,
                size_compressed,
            } => match db.insert_or_dedup_chunk(
                hash,
                nonce,
                key_id,
                *provider_id,
                storage_key,
                *size_plain,
                *size_encrypted,
                *size_compressed,
            ) {
                Ok(is_new) => RaftResponse::ChunkInserted { is_new },
                Err(e) => RaftResponse::Error(e.to_string()),
            },
            RaftRequest::DecrementChunkRef { hash } => match db.decrement_chunk_ref(hash) {
                Ok(Some((pid, sk))) => RaftResponse::ChunkDeleted {
                    provider_id: pid,
                    storage_key: sk,
                },
                Ok(None) => RaftResponse::Ok,
                Err(e) => RaftResponse::Error(e.to_string()),
            },
            RaftRequest::InsertObjectChunk {
                object_id,
                chunk_hash,
                chunk_index,
                offset,
            } => match db.insert_object_chunk(*object_id, chunk_hash, *chunk_index, *offset) {
                Ok(()) => RaftResponse::Ok,
                Err(e) => RaftResponse::Error(e.to_string()),
            },
            RaftRequest::InsertProvider {
                name,
                provider_type,
                bucket,
                region,
                weight,
            } => {
                let pt = provider_type
                    .parse()
                    .unwrap_or(enigma_core::types::ProviderType::Local);
                match db.insert_provider(name, pt, bucket, region.as_deref(), *weight) {
                    Ok(id) => RaftResponse::ProviderId(id),
                    Err(e) => RaftResponse::Error(e.to_string()),
                }
            }
            RaftRequest::CreateMultipartUpload {
                upload_id,
                namespace_id,
                key,
            } => match db.create_multipart_upload(upload_id, *namespace_id, key) {
                Ok(()) => RaftResponse::Ok,
                Err(e) => RaftResponse::Error(e.to_string()),
            },
            RaftRequest::InsertMultipartPart {
                upload_id,
                part_number,
                data,
                etag,
            } => match db.insert_multipart_part(upload_id, *part_number, data, etag) {
                Ok(()) => RaftResponse::Ok,
                Err(e) => RaftResponse::Error(e.to_string()),
            },
            RaftRequest::AbortMultipartUpload { upload_id } => {
                match db.abort_multipart_upload(upload_id) {
                    Ok(()) => RaftResponse::Ok,
                    Err(e) => RaftResponse::Error(e.to_string()),
                }
            }
        }
    }
}

impl RaftStateMachine<TypeConfig> for EnigmaStateMachine {
    type SnapshotBuilder = Self;

    async fn applied_state(
        &mut self,
    ) -> Result<(Option<LogId<u64>>, StoredMembership<u64, BasicNode>), StorageError<u64>> {
        let last_applied = self.last_applied.lock().unwrap().clone();
        let membership = self.last_membership.lock().unwrap().clone();
        Ok((last_applied, membership))
    }

    async fn apply<I>(&mut self, entries: I) -> Result<Vec<RaftResponse>, StorageError<u64>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + OptionalSend,
        I::IntoIter: OptionalSend,
    {
        let mut responses = Vec::new();

        for entry in entries {
            *self.last_applied.lock().unwrap() = Some(entry.log_id);

            match entry.payload {
                EntryPayload::Blank => {
                    responses.push(RaftResponse::Ok);
                }
                EntryPayload::Normal(req) => {
                    let resp = self.apply_request(&req);
                    responses.push(resp);
                }
                EntryPayload::Membership(mem) => {
                    *self.last_membership.lock().unwrap() =
                        StoredMembership::new(Some(entry.log_id), mem);
                    responses.push(RaftResponse::Ok);
                }
            }
        }

        Ok(responses)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        unreachable!("Snapshot builder not used in this simplified implementation")
    }

    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<Cursor<Vec<u8>>>, StorageError<u64>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        _meta: &SnapshotMeta<u64, BasicNode>,
        _snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<u64>> {
        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<TypeConfig>>, StorageError<u64>> {
        Ok(None)
    }
}

impl openraft::storage::RaftSnapshotBuilder<TypeConfig> for EnigmaStateMachine {
    async fn build_snapshot(&mut self) -> Result<Snapshot<TypeConfig>, StorageError<u64>> {
        let last_applied = self.last_applied.lock().unwrap().clone();
        let membership = self.last_membership.lock().unwrap().clone();
        let meta = SnapshotMeta {
            last_log_id: last_applied,
            last_membership: membership,
            snapshot_id: format!("snapshot-{}", last_applied.map(|l| l.index).unwrap_or(0)),
        };
        Ok(Snapshot {
            meta,
            snapshot: Box::new(Cursor::new(Vec::new())),
        })
    }
}
