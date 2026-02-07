pub mod seed;
pub mod sqlite;

#[cfg(feature = "postgres")]
pub mod postgres;

pub use sqlite::SqliteAuthStore;

#[cfg(feature = "postgres")]
pub use postgres::PostgresAuthStore;

use async_trait::async_trait;

use crate::error::AuthError;
use crate::types::*;

#[async_trait]
pub trait AuthStore: Send + Sync {
    // Users
    async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        email: Option<&str>,
    ) -> Result<User, AuthError>;
    async fn get_user_by_id(&self, id: &str) -> Result<User, AuthError>;
    async fn get_user_by_username(&self, username: &str) -> Result<User, AuthError>;
    async fn list_users(&self) -> Result<Vec<User>, AuthError>;
    async fn update_user(&self, id: &str, req: &UpdateUserRequest) -> Result<User, AuthError>;
    async fn update_password(&self, id: &str, password_hash: &str) -> Result<(), AuthError>;
    async fn delete_user(&self, id: &str) -> Result<(), AuthError>;
    async fn get_password_hash(&self, user_id: &str) -> Result<String, AuthError>;
    async fn user_count(&self) -> Result<u64, AuthError>;

    // Groups
    async fn create_group(
        &self,
        name: &str,
        description: &str,
        is_system: bool,
    ) -> Result<Group, AuthError>;
    async fn get_group(&self, id: &str) -> Result<Group, AuthError>;
    async fn get_group_by_name(&self, name: &str) -> Result<Group, AuthError>;
    async fn list_groups(&self) -> Result<Vec<Group>, AuthError>;
    async fn update_group(&self, id: &str, req: &UpdateGroupRequest) -> Result<Group, AuthError>;
    async fn delete_group(&self, id: &str) -> Result<(), AuthError>;

    // Group-Permission
    async fn add_group_permission(
        &self,
        group_id: &str,
        permission_id: &str,
    ) -> Result<(), AuthError>;
    async fn remove_group_permission(
        &self,
        group_id: &str,
        permission_id: &str,
    ) -> Result<(), AuthError>;
    async fn list_group_permissions(&self, group_id: &str) -> Result<Vec<Permission>, AuthError>;

    // User-Group
    async fn add_user_group(&self, user_id: &str, group_id: &str) -> Result<(), AuthError>;
    async fn remove_user_group(&self, user_id: &str, group_id: &str) -> Result<(), AuthError>;
    async fn list_user_groups(&self, user_id: &str) -> Result<Vec<Group>, AuthError>;
    async fn get_user_permissions(&self, user_id: &str) -> Result<Vec<String>, AuthError>;

    // Tokens
    async fn create_token(
        &self,
        user_id: &str,
        name: &str,
        token_hash: &str,
        token_prefix: &str,
        scopes: &str,
        expires_at: Option<&str>,
    ) -> Result<ApiToken, AuthError>;
    async fn verify_token(&self, token_hash: &str) -> Result<(ApiToken, User), AuthError>;
    async fn list_tokens(&self, user_id: &str) -> Result<Vec<ApiToken>, AuthError>;
    async fn revoke_token(&self, id: &str) -> Result<(), AuthError>;
    async fn touch_token(&self, id: &str) -> Result<(), AuthError>;

    // Permissions
    async fn list_permissions(&self) -> Result<Vec<Permission>, AuthError>;
    async fn create_permission(
        &self,
        action: &str,
        description: &str,
    ) -> Result<Permission, AuthError>;

    // Audit
    async fn log_audit(
        &self,
        user_id: Option<&str>,
        action: &str,
        target: Option<&str>,
        ip_addr: Option<&str>,
    ) -> Result<(), AuthError>;
    async fn list_audit(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<AuditEntry>, AuthError>;

    // Lifecycle
    async fn migrate(&self) -> Result<(), AuthError>;
    async fn seed_defaults(&self) -> Result<(), AuthError>;
}
