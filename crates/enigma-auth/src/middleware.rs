use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::error::AuthError;
use crate::jwt::{AuthClaims, verify_jwt};
use crate::permissions::has_permission;
use crate::store::AuthStore;
use crate::token::hash_token;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub groups: Vec<String>,
    pub permissions: Vec<String>,
}

#[derive(Clone)]
pub struct AuthState {
    pub jwt_secret: String,
    pub auth_store: Arc<dyn AuthStore>,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_state = parts
            .extensions
            .get::<AuthState>()
            .ok_or(AuthError::Internal("auth not configured".into()))?
            .clone();

        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok());

        let bearer = match auth_header {
            Some(h) if h.starts_with("Bearer ") => &h[7..],
            _ => return Err(AuthError::Unauthorized),
        };

        // API token (egt_ prefix)
        if bearer.starts_with("egt_") {
            let token_hash = hash_token(bearer);
            let store = &auth_state.auth_store;
            let (api_token, user) = store.verify_token(&token_hash).await?;
            // Touch last_used_at in background
            let store_clone = auth_state.auth_store.clone();
            let tid = api_token.id.clone();
            tokio::spawn(async move {
                let _ = store_clone.touch_token(&tid).await;
            });
            let permissions = store.get_user_permissions(&user.id).await?;
            let groups: Vec<String> = store
                .list_user_groups(&user.id)
                .await?
                .into_iter()
                .map(|g| g.name)
                .collect();
            return Ok(AuthUser {
                user_id: user.id,
                username: user.username,
                groups,
                permissions,
            });
        }

        // JWT
        let claims: AuthClaims = verify_jwt(bearer, &auth_state.jwt_secret)?;
        Ok(AuthUser {
            user_id: claims.sub,
            username: claims.username,
            groups: claims.groups,
            permissions: claims.permissions,
        })
    }
}

pub fn require_permission(
    user: &AuthUser,
    permission: &str,
) -> Result<(), AuthError> {
    if has_permission(&user.permissions, permission) {
        Ok(())
    } else {
        Err(AuthError::Forbidden(format!(
            "missing permission: {permission}"
        )))
    }
}
