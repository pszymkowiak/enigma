use serde::Serialize;

#[derive(Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub key_provider: String,
    pub distribution: String,
    pub compression_enabled: bool,
    pub total_providers: usize,
    pub total_chunks: u64,
    pub total_backups: usize,
    pub total_namespaces: usize,
}

#[derive(Serialize)]
pub struct ProviderResponse {
    pub id: i64,
    pub name: String,
    pub provider_type: String,
    pub bucket: String,
    pub region: Option<String>,
    pub weight: u32,
}

#[derive(Serialize)]
pub struct ChunkStatsResponse {
    pub total_chunks: u64,
    pub orphan_chunks: u64,
}

#[derive(Serialize)]
pub struct BackupResponse {
    pub id: String,
    pub source_path: String,
    pub status: String,
    pub total_files: u64,
    pub total_bytes: u64,
    pub total_chunks: u64,
    pub dedup_chunks: u64,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Serialize)]
pub struct NamespaceResponse {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub object_count: u64,
}

#[derive(Serialize)]
pub struct ObjectResponse {
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct ClusterResponse {
    pub mode: String,
    pub node_id: Option<u64>,
    pub peers: Vec<PeerResponse>,
}

#[derive(Serialize)]
pub struct PeerResponse {
    pub id: u64,
    pub addr: String,
}
