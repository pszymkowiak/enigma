use std::sync::Arc;

use tonic::{Request, Response, Status};

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
        _request: Request<tonic::Streaming<SnapshotChunk>>,
    ) -> Result<Response<ProtoSnapshotResp>, Status> {
        Err(Status::unimplemented(
            "Snapshot transfer not yet implemented",
        ))
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
