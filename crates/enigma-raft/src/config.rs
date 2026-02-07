use serde::{Deserialize, Serialize};

/// Raft cluster configuration, embedded in the TOML config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftConfig {
    /// This node's ID (1-based).
    pub node_id: u64,
    /// Directory for Raft log DB.
    pub data_dir: String,
    /// Address this node listens on for gRPC (e.g. "0.0.0.0:9000").
    pub grpc_addr: String,
    /// All peers in the cluster (including this node).
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
    /// Election timeout in milliseconds.
    #[serde(default = "default_election_timeout")]
    pub election_timeout_ms: u64,
    /// Heartbeat interval in milliseconds.
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_ms: u64,
    /// Number of log entries before triggering a snapshot.
    #[serde(default = "default_snapshot_threshold")]
    pub snapshot_threshold: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    pub id: u64,
    pub addr: String,
}

fn default_election_timeout() -> u64 {
    1000
}

fn default_heartbeat_interval() -> u64 {
    300
}

fn default_snapshot_threshold() -> u64 {
    10000
}

impl RaftConfig {
    /// Returns true if this is a single-node deployment (no Raft needed).
    pub fn is_single_node(&self) -> bool {
        self.peers.len() <= 1
    }
}
