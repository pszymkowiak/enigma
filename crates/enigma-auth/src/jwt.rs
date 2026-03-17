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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
}

pub fn create_jwt(
    user_id: &str,
    username: &str,
    groups: Vec<String>,
    permissions: Vec<String>,
    secret: &str,
) -> Result<String, AuthError> {
    if secret.len() < 32 {
        return Err(AuthError::InvalidInput(
            "JWT secret must be at least 32 bytes".to_string(),
        ));
    }
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = AuthClaims {
        sub: user_id.to_string(),
        username: username.to_string(),
        groups,
        permissions,
        exp: now + 86400,
        iat: now,
        iss: Some("enigma".to_string()),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AuthError::Internal(format!("jwt encode error: {e}")))
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<AuthClaims, AuthError> {
    if secret.len() < 32 {
        return Err(AuthError::InvalidInput(
            "JWT secret must be at least 32 bytes".to_string(),
        ));
    }
    let mut validation = Validation::default();
    validation.set_issuer(&["enigma"]);
    let data = decode::<AuthClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| AuthError::Unauthorized)?;
    Ok(data.claims)
}
