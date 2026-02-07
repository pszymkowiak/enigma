/// Trait for cluster operations — abstracts Raft from the web layer.
#[async_trait::async_trait]
pub trait ClusterHandle: Send + Sync {
    /// Returns cluster metrics as JSON (state, leader, term, membership, etc.).
    async fn metrics(&self) -> serde_json::Value;

    /// Add a new node to the cluster.
    async fn add_node(&self, node_id: u64, addr: String) -> anyhow::Result<()>;

    /// Remove a node from the cluster.
    async fn remove_node(&self, node_id: u64) -> anyhow::Result<()>;

    /// Trigger a snapshot on the leader.
    async fn trigger_snapshot(&self) -> anyhow::Result<()>;
}
