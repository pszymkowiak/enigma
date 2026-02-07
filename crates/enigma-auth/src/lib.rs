pub mod error;
pub mod jwt;
pub mod middleware;
pub mod password;
pub mod permissions;
pub mod store;
pub mod token;
pub mod types;

pub use error::AuthError;
pub use jwt::{AuthClaims, create_jwt, verify_jwt};
pub use middleware::AuthUser;
pub use password::{hash_password, verify_password};
pub use permissions::{has_permission, PERMISSIONS};
pub use store::{AuthStore, SqliteAuthStore};
pub use token::{generate_api_token, hash_token};
pub use types::*;

#[cfg(feature = "postgres")]
pub use store::PostgresAuthStore;
