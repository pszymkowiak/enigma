use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::models::{BackupResponse, ChunkStatsResponse, ProviderResponse};
use crate::state::AppState;

pub async fn get_providers(State(state): State<Arc<AppState>>) -> Json<Vec<ProviderResponse>> {
    let db = state.db.lock().unwrap();
    let providers = db.list_providers().unwrap_or_default();
    Json(
        providers
            .iter()
            .map(|p| ProviderResponse {
                id: p.id,
                name: p.name.clone(),
                provider_type: p.provider_type.to_string(),
                bucket: p.bucket.clone(),
                region: p.region.clone(),
                weight: p.weight,
            })
            .collect(),
    )
}

pub async fn get_chunk_stats(State(state): State<Arc<AppState>>) -> Json<ChunkStatsResponse> {
    let db = state.db.lock().unwrap();
    let (total, orphans) = db.chunk_stats().unwrap_or((0, 0));
    Json(ChunkStatsResponse {
        total_chunks: total,
        orphan_chunks: orphans,
    })
}

pub async fn get_backups(State(state): State<Arc<AppState>>) -> Json<Vec<BackupResponse>> {
    let db = state.db.lock().unwrap();
    let backups = db.list_backups().unwrap_or_default();
    Json(
        backups
            .iter()
            .map(|b| BackupResponse {
                id: b.id.clone(),
                source_path: b.source_path.clone(),
                status: b.status.to_string(),
                total_files: b.total_files,
                total_bytes: b.total_bytes,
                total_chunks: b.total_chunks,
                dedup_chunks: b.dedup_chunks,
                created_at: b.created_at.clone(),
                completed_at: b.completed_at.clone(),
            })
            .collect(),
    )
}
