use std::io::Cursor;
use std::sync::Arc;

use openraft::{BasicNode, Snapshot, SnapshotMeta, Vote};
use tonic::{Request, Response, Status};
use tokio_stream::StreamExt;

use crate::EnigmaRaft;
use crate::proto::raft_service_server::RaftService;
use crate::proto::{
    AppendEntriesRequest as ProtoAppendReq, AppendEntriesResponse as ProtoAppendResp,
    InstallSnapshotResponse as ProtoSnapshotResp, SnapshotChunk, VoteRequest as ProtoVoteReq,
    VoteResponse as ProtoVoteResp, WriteRequest, WriteResponse,
};
use crate::types::RaftRequest;

/// gRPC server implementing the Raft service.
pub struct EnigmaRaftGrpcServer {
    pub raft: Arc<EnigmaRaft>,
}

impl EnigmaRaftGrpcServer {
    pub fn new(raft: Arc<EnigmaRaft>) -> Self {
        Self { raft }
    }
}

#[tonic::async_trait]
impl RaftService for EnigmaRaftGrpcServer {
    async fn append_entries(
        &self,
        request: Request<ProtoAppendReq>,
    ) -> Result<Response<ProtoAppendResp>, Status> {
        let req: openraft::raft::AppendEntriesRequest<crate::TypeConfig> =
            serde_json::from_slice(&request.into_inner().data)
                .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let resp = self
            .raft
            .append_entries(req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let data = serde_json::to_vec(&resp).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(ProtoAppendResp { data }))
    }

    async fn vote(
        &self,
        request: Request<ProtoVoteReq>,
    ) -> Result<Response<ProtoVoteResp>, Status> {
        let req: openraft::raft::VoteRequest<u64> =
            serde_json::from_slice(&request.into_inner().data)
                .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let resp = self
            .raft
            .vote(req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let data = serde_json::to_vec(&resp).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(ProtoVoteResp { data }))
    }

    async fn install_snapshot(
        &self,
        request: Request<tonic::Streaming<SnapshotChunk>>,
    ) -> Result<Response<ProtoSnapshotResp>, Status> {
        // Receive all chunks from the stream
        let mut stream = request.into_inner();
        let mut payload = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            payload.extend_from_slice(&chunk.data);
        }

        // Parse: [8 bytes meta_len LE][meta JSON][DB bytes]
        if payload.len() < 8 {
            return Err(Status::invalid_argument("Snapshot payload too short"));
        }

        let meta_len =
            u64::from_le_bytes(payload[0..8].try_into().unwrap()) as usize;

        if payload.len() < 8 + meta_len {
            return Err(Status::invalid_argument("Snapshot metadata truncated"));
        }

        let (vote, meta): (Vote<u64>, SnapshotMeta<u64, BasicNode>) =
            serde_json::from_slice(&payload[8..8 + meta_len])
                .map_err(|e| Status::invalid_argument(format!("Invalid snapshot metadata: {e}")))?;

        let db_bytes = payload[8 + meta_len..].to_vec();

        tracing::info!(
            snapshot_id = %meta.snapshot_id,
            last_log_id = ?meta.last_log_id,
            db_bytes = db_bytes.len(),
            "Received snapshot via gRPC"
        );

        let snapshot = Snapshot {
            meta,
            snapshot: Box::new(Cursor::new(db_bytes)),
        };

        let resp = self
            .raft
            .install_full_snapshot(vote, snapshot)
            .await
            .map_err(|e| Status::internal(format!("install_full_snapshot failed: {e}")))?;

        let data =
            serde_json::to_vec(&resp).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(ProtoSnapshotResp { data }))
    }

    async fn forward_write(
        &self,
        request: Request<WriteRequest>,
    ) -> Result<Response<WriteResponse>, Status> {
        let req: RaftRequest = serde_json::from_slice(&request.into_inner().data)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let resp = self
            .raft
            .client_write(req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let data = serde_json::to_vec(&resp.data).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(WriteResponse { data }))
    }
}
