use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use openraft::error::{RPCError, RaftError, Unreachable};
use openraft::network::{RPCOption, RaftNetwork, RaftNetworkFactory};
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    SnapshotResponse, VoteRequest, VoteResponse,
};
use openraft::{BasicNode, Snapshot, Vote};

use crate::TypeConfig;
use crate::proto::raft_service_client::RaftServiceClient;

/// Factory that creates gRPC network connections to peers.
pub struct EnigmaNetworkFactory {
    pub peers: Arc<Mutex<HashMap<u64, String>>>,
}

impl EnigmaNetworkFactory {
    pub fn new(peers: HashMap<u64, String>) -> Self {
        Self {
            peers: Arc::new(Mutex::new(peers)),
        }
    }
}

impl RaftNetworkFactory<TypeConfig> for EnigmaNetworkFactory {
    type Network = EnigmaNetwork;

    async fn new_client(&mut self, target: u64, _node: &BasicNode) -> Self::Network {
        let addr = {
            let peers = self.peers.lock().unwrap();
            peers.get(&target).cloned().unwrap_or_default()
        };
        EnigmaNetwork { target, addr }
    }
}

/// A gRPC client to a single Raft peer.
pub struct EnigmaNetwork {
    #[allow(dead_code)]
    target: u64,
    addr: String,
}

impl EnigmaNetwork {
    async fn client(
        &self,
    ) -> Result<
        RaftServiceClient<tonic::transport::Channel>,
        RPCError<u64, BasicNode, RaftError<u64>>,
    > {
        let endpoint = format!("http://{}", self.addr);
        RaftServiceClient::connect(endpoint)
            .await
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))
    }
}

impl RaftNetwork<TypeConfig> for EnigmaNetwork {
    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<u64>, RPCError<u64, BasicNode, RaftError<u64>>> {
        let data =
            serde_json::to_vec(&rpc).map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let mut client = self.client().await?;
        let resp = client
            .append_entries(crate::proto::AppendEntriesRequest { data })
            .await
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let inner = resp.into_inner();
        serde_json::from_slice(&inner.data).map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))
    }

    async fn install_snapshot(
        &mut self,
        _rpc: InstallSnapshotRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<
        InstallSnapshotResponse<u64>,
        RPCError<u64, BasicNode, RaftError<u64, openraft::error::InstallSnapshotError>>,
    > {
        Err(RPCError::Unreachable(Unreachable::new(
            &std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Snapshot transfer not yet implemented",
            ),
        )))
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<u64>,
        _option: RPCOption,
    ) -> Result<VoteResponse<u64>, RPCError<u64, BasicNode, RaftError<u64>>> {
        let data =
            serde_json::to_vec(&rpc).map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let mut client = self.client().await?;
        let resp = client
            .vote(crate::proto::VoteRequest { data })
            .await
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let inner = resp.into_inner();
        serde_json::from_slice(&inner.data).map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))
    }

    async fn full_snapshot(
        &mut self,
        _vote: Vote<u64>,
        _snapshot: Snapshot<TypeConfig>,
        _cancel: impl std::future::Future<Output = openraft::error::ReplicationClosed>
        + openraft::OptionalSend
        + 'static,
        _option: RPCOption,
    ) -> Result<
        SnapshotResponse<u64>,
        openraft::error::StreamingError<TypeConfig, openraft::error::Fatal<u64>>,
    > {
        Err(openraft::error::StreamingError::Unreachable(
            Unreachable::new(&std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Snapshot transfer not yet implemented",
            )),
        ))
    }
}
