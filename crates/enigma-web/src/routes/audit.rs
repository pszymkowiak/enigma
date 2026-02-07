use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};

use enigma_auth::middleware::require_permission;
use enigma_auth::AuthUser;
use enigma_auth::error::AuthError;

use crate::models::{AuditQuery, AuditResponse};
use crate::state::AppState;

pub async fn list_audit(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditResponse>>, AuthError> {
    require_permission(&auth_user, "audit:read")?;

    let limit = query.limit.unwrap_or(50).min(500);
    let offset = query.offset.unwrap_or(0);

    let entries = state.auth_store.list_audit(limit, offset).await?;
    Ok(Json(
        entries
            .into_iter()
            .map(|e| AuditResponse {
                id: e.id,
                user_id: e.user_id,
                action: e.action,
                target: e.target,
                ip_addr: e.ip_addr,
                created_at: e.created_at,
            })
            .collect(),
    ))
}
