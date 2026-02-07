use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use enigma_auth::middleware::require_permission;
use enigma_auth::AuthUser;
use enigma_auth::error::AuthError;

use crate::models::PermissionResponse;
use crate::state::AppState;

pub async fn list_permissions(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PermissionResponse>>, AuthError> {
    require_permission(&auth_user, "groups:read")?;

    let perms = state.auth_store.list_permissions().await?;
    Ok(Json(
        perms
            .into_iter()
            .map(|p| PermissionResponse {
                id: p.id,
                action: p.action,
                description: p.description,
            })
            .collect(),
    ))
}
