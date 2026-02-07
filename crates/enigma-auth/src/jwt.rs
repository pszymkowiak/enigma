use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use crate::error::AuthError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthClaims {
    pub sub: String,
    pub username: String,
    pub groups: Vec<String>,
    pub permissions: Vec<String>,
    pub exp: usize,
    pub iat: usize,
}

pub fn create_jwt(
    user_id: &str,
    username: &str,
    groups: Vec<String>,
    permissions: Vec<String>,
    secret: &str,
) -> Result<String, AuthError> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = AuthClaims {
        sub: user_id.to_string(),
        username: username.to_string(),
        groups,
        permissions,
        exp: now + 86400,
        iat: now,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AuthError::Internal(format!("jwt encode error: {e}")))
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<AuthClaims, AuthError> {
    let data = decode::<AuthClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AuthError::Unauthorized)?;
    Ok(data.claims)
}
