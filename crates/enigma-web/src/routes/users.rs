use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};

use enigma_auth::middleware::require_permission;
use enigma_auth::AuthUser;
use enigma_auth::error::AuthError;

use crate::models::{GroupResponse, PermissionResponse, UserResponse};
use crate::state::AppState;

async fn user_to_response(
    state: &AppState,
    user: enigma_auth::User,
) -> Result<UserResponse, AuthError> {
    let groups = state.auth_store.list_user_groups(&user.id).await?;
    let mut group_responses = Vec::new();
    for g in groups {
        let perms = state.auth_store.list_group_permissions(&g.id).await?;
        group_responses.push(GroupResponse {
            id: g.id,
            name: g.name,
            description: g.description,
            is_system: g.is_system,
            permissions: perms
                .into_iter()
                .map(|p| PermissionResponse {
                    id: p.id,
                    action: p.action,
                    description: p.description,
                })
                .collect(),
            created_at: g.created_at,
        });
    }
    Ok(UserResponse {
        id: user.id,
        username: user.username,
        email: user.email,
        is_active: user.is_active,
        groups: group_responses,
        created_at: user.created_at,
        updated_at: user.updated_at,
    })
}

pub async fn list_users(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<UserResponse>>, AuthError> {
    require_permission(&auth_user, "users:read")?;

    let users = state.auth_store.list_users().await?;
    let mut result = Vec::new();
    for u in users {
        result.push(user_to_response(&state, u).await?);
    }
    Ok(Json(result))
}

pub async fn get_user(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<UserResponse>, AuthError> {
    require_permission(&auth_user, "users:read")?;

    let user = state.auth_store.get_user_by_id(&id).await?;
    Ok(Json(user_to_response(&state, user).await?))
}

pub async fn create_user(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(req): Json<enigma_auth::CreateUserRequest>,
) -> Result<Json<UserResponse>, AuthError> {
    require_permission(&auth_user, "users:write")?;

    if req.username.is_empty() || req.password.len() < 4 {
        return Err(AuthError::InvalidInput(
            "username required, password min 4 chars".into(),
        ));
    }

    let password_hash = enigma_auth::hash_password(&req.password)?;
    let user = state
        .auth_store
        .create_user(&req.username, &password_hash, req.email.as_deref())
        .await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "user.create",
            Some(&user.username),
            None,
        )
        .await;

    Ok(Json(user_to_response(&state, user).await?))
}

pub async fn update_user(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<enigma_auth::UpdateUserRequest>,
) -> Result<Json<UserResponse>, AuthError> {
    require_permission(&auth_user, "users:write")?;

    let user = state.auth_store.update_user(&id, &req).await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "user.update",
            Some(&user.username),
            None,
        )
        .await;

    Ok(Json(user_to_response(&state, user).await?))
}

pub async fn delete_user(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AuthError> {
    require_permission(&auth_user, "users:write")?;

    if auth_user.user_id == id {
        return Err(AuthError::Forbidden("cannot delete yourself".into()));
    }

    let user = state.auth_store.get_user_by_id(&id).await?;
    state.auth_store.delete_user(&id).await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "user.delete",
            Some(&user.username),
            None,
        )
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn update_password(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<enigma_auth::UpdatePasswordRequest>,
) -> Result<Json<serde_json::Value>, AuthError> {
    // Users can change their own password, admins can change anyone's
    if auth_user.user_id != id {
        require_permission(&auth_user, "users:write")?;
    }

    if req.password.len() < 4 {
        return Err(AuthError::InvalidInput("password min 4 chars".into()));
    }

    let password_hash = enigma_auth::hash_password(&req.password)?;
    state.auth_store.update_password(&id, &password_hash).await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "user.password_change",
            Some(&id),
            None,
        )
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn list_user_groups(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<GroupResponse>>, AuthError> {
    require_permission(&auth_user, "users:read")?;

    let groups = state.auth_store.list_user_groups(&id).await?;
    let mut result = Vec::new();
    for g in groups {
        let perms = state.auth_store.list_group_permissions(&g.id).await?;
        result.push(GroupResponse {
            id: g.id,
            name: g.name,
            description: g.description,
            is_system: g.is_system,
            permissions: perms
                .into_iter()
                .map(|p| PermissionResponse {
                    id: p.id,
                    action: p.action,
                    description: p.description,
                })
                .collect(),
            created_at: g.created_at,
        });
    }
    Ok(Json(result))
}

pub async fn add_user_group(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<enigma_auth::UserGroupRequest>,
) -> Result<Json<serde_json::Value>, AuthError> {
    require_permission(&auth_user, "users:write")?;

    state.auth_store.add_user_group(&id, &req.group_id).await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "user.group.add",
            Some(&format!("{}:{}", id, req.group_id)),
            None,
        )
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn remove_user_group(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path((id, group_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, AuthError> {
    require_permission(&auth_user, "users:write")?;

    state.auth_store.remove_user_group(&id, &group_id).await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "user.group.remove",
            Some(&format!("{id}:{group_id}")),
            None,
        )
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}
