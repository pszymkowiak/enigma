use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};

use enigma_auth::middleware::require_permission;
use enigma_auth::AuthUser;
use enigma_auth::error::AuthError;

use crate::models::{GroupResponse, PermissionResponse};
use crate::state::AppState;

async fn group_to_response(
    state: &AppState,
    group: enigma_auth::Group,
) -> Result<GroupResponse, AuthError> {
    let perms = state.auth_store.list_group_permissions(&group.id).await?;
    Ok(GroupResponse {
        id: group.id,
        name: group.name,
        description: group.description,
        is_system: group.is_system,
        permissions: perms
            .into_iter()
            .map(|p| PermissionResponse {
                id: p.id,
                action: p.action,
                description: p.description,
            })
            .collect(),
        created_at: group.created_at,
    })
}

pub async fn list_groups(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<GroupResponse>>, AuthError> {
    require_permission(&auth_user, "groups:read")?;

    let groups = state.auth_store.list_groups().await?;
    let mut result = Vec::new();
    for g in groups {
        result.push(group_to_response(&state, g).await?);
    }
    Ok(Json(result))
}

pub async fn get_group(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<GroupResponse>, AuthError> {
    require_permission(&auth_user, "groups:read")?;

    let group = state.auth_store.get_group(&id).await?;
    Ok(Json(group_to_response(&state, group).await?))
}

pub async fn create_group(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(req): Json<enigma_auth::CreateGroupRequest>,
) -> Result<Json<GroupResponse>, AuthError> {
    require_permission(&auth_user, "groups:write")?;

    if req.name.is_empty() {
        return Err(AuthError::InvalidInput("group name required".into()));
    }

    let group = state
        .auth_store
        .create_group(&req.name, req.description.as_deref().unwrap_or(""), false)
        .await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "group.create",
            Some(&group.name),
            None,
        )
        .await;

    Ok(Json(group_to_response(&state, group).await?))
}

pub async fn update_group(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<enigma_auth::UpdateGroupRequest>,
) -> Result<Json<GroupResponse>, AuthError> {
    require_permission(&auth_user, "groups:write")?;

    let group = state.auth_store.update_group(&id, &req).await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "group.update",
            Some(&group.name),
            None,
        )
        .await;

    Ok(Json(group_to_response(&state, group).await?))
}

pub async fn delete_group(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AuthError> {
    require_permission(&auth_user, "groups:write")?;

    let group = state.auth_store.get_group(&id).await?;
    state.auth_store.delete_group(&id).await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "group.delete",
            Some(&group.name),
            None,
        )
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn list_group_permissions(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<PermissionResponse>>, AuthError> {
    require_permission(&auth_user, "groups:read")?;

    let perms = state.auth_store.list_group_permissions(&id).await?;
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

pub async fn add_group_permission(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<enigma_auth::GroupPermissionRequest>,
) -> Result<Json<serde_json::Value>, AuthError> {
    require_permission(&auth_user, "groups:write")?;

    state
        .auth_store
        .add_group_permission(&id, &req.permission_id)
        .await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "group.permission.add",
            Some(&format!("{id}:{}", req.permission_id)),
            None,
        )
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn remove_group_permission(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path((id, permission_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, AuthError> {
    require_permission(&auth_user, "groups:write")?;

    state
        .auth_store
        .remove_group_permission(&id, &permission_id)
        .await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "group.permission.remove",
            Some(&format!("{id}:{permission_id}")),
            None,
        )
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}
