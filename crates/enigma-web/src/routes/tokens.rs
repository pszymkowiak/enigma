use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};

use enigma_auth::middleware::require_permission;
use enigma_auth::AuthUser;
use enigma_auth::error::AuthError;

use crate::models::{CreateTokenResponse, TokenResponse};
use crate::state::AppState;

pub async fn list_tokens(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TokenResponse>>, AuthError> {
    require_permission(&auth_user, "tokens:own")?;

    let tokens = state.auth_store.list_tokens(&auth_user.user_id).await?;
    Ok(Json(
        tokens
            .into_iter()
            .map(|t| TokenResponse {
                id: t.id,
                name: t.name,
                token_prefix: t.token_prefix,
                scopes: t.scopes,
                expires_at: t.expires_at,
                last_used_at: t.last_used_at,
                created_at: t.created_at,
            })
            .collect(),
    ))
}

pub async fn create_token(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(req): Json<enigma_auth::CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, AuthError> {
    require_permission(&auth_user, "tokens:own")?;

    if req.name.is_empty() {
        return Err(AuthError::InvalidInput("token name required".into()));
    }

    let raw_token = enigma_auth::generate_api_token();
    let token_hash = enigma_auth::hash_token(&raw_token);
    let token_prefix = &raw_token[..12]; // "egt_" + 8 hex chars
    let scopes = req.scopes.as_deref().unwrap_or("*");

    let expires_at = req.expires_in_days.map(|days| {
        let dt = chrono::Utc::now() + chrono::Duration::days(days as i64);
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    });

    let api_token = state
        .auth_store
        .create_token(
            &auth_user.user_id,
            &req.name,
            &token_hash,
            token_prefix,
            scopes,
            expires_at.as_deref(),
        )
        .await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "token.create",
            Some(&req.name),
            None,
        )
        .await;

    Ok(Json(CreateTokenResponse {
        token: TokenResponse {
            id: api_token.id,
            name: api_token.name,
            token_prefix: api_token.token_prefix,
            scopes: api_token.scopes,
            expires_at: api_token.expires_at,
            last_used_at: api_token.last_used_at,
            created_at: api_token.created_at,
        },
        raw_token,
    }))
}

pub async fn revoke_token(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AuthError> {
    require_permission(&auth_user, "tokens:own")?;

    // Check token ownership unless admin
    if !enigma_auth::has_permission(&auth_user.permissions, "tokens:admin") {
        let tokens = state.auth_store.list_tokens(&auth_user.user_id).await?;
        if !tokens.iter().any(|t| t.id == id) {
            return Err(AuthError::Forbidden("not your token".into()));
        }
    }

    state.auth_store.revoke_token(&id).await?;

    let _ = state
        .auth_store
        .log_audit(
            Some(&auth_user.user_id),
            "token.revoke",
            Some(&id),
            None,
        )
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}
