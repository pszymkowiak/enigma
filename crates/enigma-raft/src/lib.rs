pub mod config;
pub mod grpc_server;
pub mod log_store;
pub mod network;
pub mod state_machine;
pub mod types;

pub mod proto {
    tonic::include_proto!("enigma.raft");
}

use std::io::Cursor;

use openraft::BasicNode;

openraft::declare_raft_types!(
    pub TypeConfig:
        D = types::RaftRequest,
        R = types::RaftResponse,
        NodeId = u64,
        Node = BasicNode,
        Entry = openraft::Entry<TypeConfig>,
        SnapshotData = Cursor<Vec<u8>>,
);

pub type EnigmaRaft = openraft::Raft<TypeConfig>;
