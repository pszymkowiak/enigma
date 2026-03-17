use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::models::StatusResponse;
use crate::state::AppState;

pub async fn get_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StatusResponse>, (StatusCode, &'static str)> {
    let db = state
        .db
        .lock()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?;
    let providers = db.list_providers().unwrap_or_default();
    let (total_chunks, _) = db.chunk_stats().unwrap_or((0, 0));
    let backups = db.list_backups().unwrap_or_default();
    let namespaces = db.list_namespaces().unwrap_or_default();

    Ok(Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        key_provider: state.config.key_provider.clone(),
        distribution: format!("{:?}", state.config.distribution),
        compression_enabled: state.config.compression.enabled,
        total_providers: providers.len(),
        total_chunks,
        total_backups: backups.len(),
        total_namespaces: namespaces.len(),
    }))
}
