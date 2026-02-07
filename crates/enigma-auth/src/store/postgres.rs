#![cfg(feature = "postgres")]

use async_trait::async_trait;
use sqlx::PgPool;

use super::AuthStore;
use crate::error::AuthError;
use crate::types::*;

pub struct PostgresAuthStore {
    pool: PgPool,
}

impl PostgresAuthStore {
    pub async fn new(database_url: &str) -> Result<Self, AuthError> {
        let pool = PgPool::connect(database_url)
            .await
            .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(Self { pool })
    }
}

const MIGRATE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS auth_users (
    id TEXT PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    email TEXT UNIQUE,
    password_hash TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS auth_groups (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS auth_permissions (
    id TEXT PRIMARY KEY,
    action TEXT UNIQUE NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS auth_group_permissions (
    group_id TEXT NOT NULL REFERENCES auth_groups(id) ON DELETE CASCADE,
    permission_id TEXT NOT NULL REFERENCES auth_permissions(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, permission_id)
);

CREATE TABLE IF NOT EXISTS auth_user_groups (
    user_id TEXT NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    group_id TEXT NOT NULL REFERENCES auth_groups(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, group_id)
);

CREATE TABLE IF NOT EXISTS auth_api_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT UNIQUE NOT NULL,
    token_prefix TEXT NOT NULL,
    scopes TEXT NOT NULL DEFAULT '*',
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS auth_audit_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT,
    action TEXT NOT NULL,
    target TEXT,
    ip_addr TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
"#;

#[async_trait]
impl AuthStore for PostgresAuthStore {
    async fn migrate(&self) -> Result<(), AuthError> {
        sqlx::raw_sql(MIGRATE_SQL)
            .execute(&self.pool)
            .await
            .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(())
    }

    async fn seed_defaults(&self) -> Result<(), AuthError> {
        super::seed::seed_defaults(self).await
    }

    // --- Users ---

    async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        email: Option<&str>,
    ) -> Result<User, AuthError> {
        let id = uuid::Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, (String, String, Option<String>, bool, String, String)>(
            "INSERT INTO auth_users (id, username, email, password_hash)
             VALUES ($1, $2, $3, $4)
             RETURNING id, username, email, is_active, created_at::text, updated_at::text",
        )
        .bind(&id)
        .bind(username)
        .bind(email)
        .bind(password_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate") || e.to_string().contains("unique") {
                AuthError::Duplicate(format!("user '{username}' already exists"))
            } else {
                AuthError::Database(e.to_string())
            }
        })?;
        Ok(User {
            id: row.0,
            username: row.1,
            email: row.2,
            is_active: row.3,
            created_at: row.4,
            updated_at: row.5,
        })
    }

    async fn get_user_by_id(&self, id: &str) -> Result<User, AuthError> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, bool, String, String)>(
            "SELECT id, username, email, is_active, created_at::text, updated_at::text
             FROM auth_users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?
        .ok_or_else(|| AuthError::NotFound("user not found".into()))?;
        Ok(User {
            id: row.0,
            username: row.1,
            email: row.2,
            is_active: row.3,
            created_at: row.4,
            updated_at: row.5,
        })
    }

    async fn get_user_by_username(&self, username: &str) -> Result<User, AuthError> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, bool, String, String)>(
            "SELECT id, username, email, is_active, created_at::text, updated_at::text
             FROM auth_users WHERE username = $1",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?
        .ok_or_else(|| AuthError::NotFound("user not found".into()))?;
        Ok(User {
            id: row.0,
            username: row.1,
            email: row.2,
            is_active: row.3,
            created_at: row.4,
            updated_at: row.5,
        })
    }

    async fn list_users(&self) -> Result<Vec<User>, AuthError> {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, bool, String, String)>(
            "SELECT id, username, email, is_active, created_at::text, updated_at::text
             FROM auth_users ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| User {
                id: r.0,
                username: r.1,
                email: r.2,
                is_active: r.3,
                created_at: r.4,
                updated_at: r.5,
            })
            .collect())
    }

    async fn update_user(&self, id: &str, req: &UpdateUserRequest) -> Result<User, AuthError> {
        if let Some(ref email) = req.email {
            sqlx::query("UPDATE auth_users SET email = $1, updated_at = NOW() WHERE id = $2")
                .bind(email)
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| AuthError::Database(e.to_string()))?;
        }
        if let Some(is_active) = req.is_active {
            sqlx::query("UPDATE auth_users SET is_active = $1, updated_at = NOW() WHERE id = $2")
                .bind(is_active)
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| AuthError::Database(e.to_string()))?;
        }
        self.get_user_by_id(id).await
    }

    async fn update_password(&self, id: &str, password_hash: &str) -> Result<(), AuthError> {
        let result = sqlx::query(
            "UPDATE auth_users SET password_hash = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(password_hash)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(AuthError::NotFound("user not found".into()));
        }
        Ok(())
    }

    async fn delete_user(&self, id: &str) -> Result<(), AuthError> {
        let result = sqlx::query("DELETE FROM auth_users WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AuthError::Database(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(AuthError::NotFound("user not found".into()));
        }
        Ok(())
    }

    async fn get_password_hash(&self, user_id: &str) -> Result<String, AuthError> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT password_hash FROM auth_users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?
        .ok_or_else(|| AuthError::NotFound("user not found".into()))?;
        Ok(row.0)
    }

    async fn user_count(&self) -> Result<u64, AuthError> {
        let row = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM auth_users")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(row.0 as u64)
    }

    // --- Groups ---

    async fn create_group(
        &self,
        name: &str,
        description: &str,
        is_system: bool,
    ) -> Result<Group, AuthError> {
        let id = uuid::Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, (String, String, String, bool, String)>(
            "INSERT INTO auth_groups (id, name, description, is_system)
             VALUES ($1, $2, $3, $4)
             RETURNING id, name, description, is_system, created_at::text",
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(is_system)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate") || e.to_string().contains("unique") {
                AuthError::Duplicate(format!("group '{name}' already exists"))
            } else {
                AuthError::Database(e.to_string())
            }
        })?;
        Ok(Group {
            id: row.0,
            name: row.1,
            description: row.2,
            is_system: row.3,
            created_at: row.4,
        })
    }

    async fn get_group(&self, id: &str) -> Result<Group, AuthError> {
        let row = sqlx::query_as::<_, (String, String, String, bool, String)>(
            "SELECT id, name, description, is_system, created_at::text FROM auth_groups WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?
        .ok_or_else(|| AuthError::NotFound("group not found".into()))?;
        Ok(Group {
            id: row.0,
            name: row.1,
            description: row.2,
            is_system: row.3,
            created_at: row.4,
        })
    }

    async fn get_group_by_name(&self, name: &str) -> Result<Group, AuthError> {
        let row = sqlx::query_as::<_, (String, String, String, bool, String)>(
            "SELECT id, name, description, is_system, created_at::text FROM auth_groups WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?
        .ok_or_else(|| AuthError::NotFound("group not found".into()))?;
        Ok(Group {
            id: row.0,
            name: row.1,
            description: row.2,
            is_system: row.3,
            created_at: row.4,
        })
    }

    async fn list_groups(&self) -> Result<Vec<Group>, AuthError> {
        let rows = sqlx::query_as::<_, (String, String, String, bool, String)>(
            "SELECT id, name, description, is_system, created_at::text FROM auth_groups ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| Group {
                id: r.0,
                name: r.1,
                description: r.2,
                is_system: r.3,
                created_at: r.4,
            })
            .collect())
    }

    async fn update_group(&self, id: &str, req: &UpdateGroupRequest) -> Result<Group, AuthError> {
        if let Some(ref desc) = req.description {
            let result =
                sqlx::query("UPDATE auth_groups SET description = $1 WHERE id = $2")
                    .bind(desc)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| AuthError::Database(e.to_string()))?;
            if result.rows_affected() == 0 {
                return Err(AuthError::NotFound("group not found".into()));
            }
        }
        self.get_group(id).await
    }

    async fn delete_group(&self, id: &str) -> Result<(), AuthError> {
        let row = sqlx::query_as::<_, (bool,)>(
            "SELECT is_system FROM auth_groups WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?
        .ok_or_else(|| AuthError::NotFound("group not found".into()))?;
        if row.0 {
            return Err(AuthError::Forbidden("cannot delete system group".into()));
        }
        sqlx::query("DELETE FROM auth_groups WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(())
    }

    // --- Group-Permission ---

    async fn add_group_permission(
        &self,
        group_id: &str,
        permission_id: &str,
    ) -> Result<(), AuthError> {
        sqlx::query(
            "INSERT INTO auth_group_permissions (group_id, permission_id)
             VALUES ($1, $2)
             ON CONFLICT DO NOTHING",
        )
        .bind(group_id)
        .bind(permission_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(())
    }

    async fn remove_group_permission(
        &self,
        group_id: &str,
        permission_id: &str,
    ) -> Result<(), AuthError> {
        sqlx::query(
            "DELETE FROM auth_group_permissions WHERE group_id = $1 AND permission_id = $2",
        )
        .bind(group_id)
        .bind(permission_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_group_permissions(&self, group_id: &str) -> Result<Vec<crate::types::Permission>, AuthError> {
        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT p.id, p.action, p.description, p.created_at::text
             FROM auth_permissions p
             JOIN auth_group_permissions gp ON gp.permission_id = p.id
             WHERE gp.group_id = $1
             ORDER BY p.action",
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| crate::types::Permission {
                id: r.0,
                action: r.1,
                description: r.2,
                created_at: r.3,
            })
            .collect())
    }

    // --- User-Group ---

    async fn add_user_group(&self, user_id: &str, group_id: &str) -> Result<(), AuthError> {
        sqlx::query(
            "INSERT INTO auth_user_groups (user_id, group_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(group_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(())
    }

    async fn remove_user_group(&self, user_id: &str, group_id: &str) -> Result<(), AuthError> {
        sqlx::query("DELETE FROM auth_user_groups WHERE user_id = $1 AND group_id = $2")
            .bind(user_id)
            .bind(group_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_user_groups(&self, user_id: &str) -> Result<Vec<Group>, AuthError> {
        let rows = sqlx::query_as::<_, (String, String, String, bool, String)>(
            "SELECT g.id, g.name, g.description, g.is_system, g.created_at::text
             FROM auth_groups g
             JOIN auth_user_groups ug ON ug.group_id = g.id
             WHERE ug.user_id = $1
             ORDER BY g.name",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| Group {
                id: r.0,
                name: r.1,
                description: r.2,
                is_system: r.3,
                created_at: r.4,
            })
            .collect())
    }

    async fn get_user_permissions(&self, user_id: &str) -> Result<Vec<String>, AuthError> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT DISTINCT p.action
             FROM auth_permissions p
             JOIN auth_group_permissions gp ON gp.permission_id = p.id
             JOIN auth_user_groups ug ON ug.group_id = gp.group_id
             WHERE ug.user_id = $1
             ORDER BY p.action",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    // --- Tokens ---

    async fn create_token(
        &self,
        user_id: &str,
        name: &str,
        token_hash: &str,
        token_prefix: &str,
        scopes: &str,
        expires_at: Option<&str>,
    ) -> Result<ApiToken, AuthError> {
        let id = uuid::Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, (String, String, String, String, String, Option<String>, Option<String>, String)>(
            "INSERT INTO auth_api_tokens (id, user_id, name, token_hash, token_prefix, scopes, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7::timestamptz)
             RETURNING id, user_id, name, token_prefix, scopes, expires_at::text, last_used_at::text, created_at::text",
        )
        .bind(&id)
        .bind(user_id)
        .bind(name)
        .bind(token_hash)
        .bind(token_prefix)
        .bind(scopes)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(ApiToken {
            id: row.0,
            user_id: row.1,
            name: row.2,
            token_prefix: row.3,
            scopes: row.4,
            expires_at: row.5,
            last_used_at: row.6,
            created_at: row.7,
        })
    }

    async fn verify_token(&self, token_hash: &str) -> Result<(ApiToken, User), AuthError> {
        let row = sqlx::query_as::<_, (
            String, String, String, String, String, Option<String>, Option<String>, String,
            String, String, Option<String>, bool, String, String,
        )>(
            "SELECT t.id, t.user_id, t.name, t.token_prefix, t.scopes, t.expires_at::text, t.last_used_at::text, t.created_at::text,
                    u.id, u.username, u.email, u.is_active, u.created_at::text, u.updated_at::text
             FROM auth_api_tokens t
             JOIN auth_users u ON u.id = t.user_id
             WHERE t.token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?
        .ok_or(AuthError::Unauthorized)?;

        let token = ApiToken {
            id: row.0,
            user_id: row.1,
            name: row.2,
            token_prefix: row.3,
            scopes: row.4,
            expires_at: row.5,
            last_used_at: row.6,
            created_at: row.7,
        };
        let user = User {
            id: row.8,
            username: row.9,
            email: row.10,
            is_active: row.11,
            created_at: row.12,
            updated_at: row.13,
        };

        if !user.is_active {
            return Err(AuthError::Unauthorized);
        }
        if let Some(ref exp) = token.expires_at {
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            if exp < &now {
                return Err(AuthError::Unauthorized);
            }
        }

        Ok((token, user))
    }

    async fn list_tokens(&self, user_id: &str) -> Result<Vec<ApiToken>, AuthError> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, Option<String>, Option<String>, String)>(
            "SELECT id, user_id, name, token_prefix, scopes, expires_at::text, last_used_at::text, created_at::text
             FROM auth_api_tokens WHERE user_id = $1 ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| ApiToken {
                id: r.0,
                user_id: r.1,
                name: r.2,
                token_prefix: r.3,
                scopes: r.4,
                expires_at: r.5,
                last_used_at: r.6,
                created_at: r.7,
            })
            .collect())
    }

    async fn revoke_token(&self, id: &str) -> Result<(), AuthError> {
        let result = sqlx::query("DELETE FROM auth_api_tokens WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AuthError::Database(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(AuthError::NotFound("token not found".into()));
        }
        Ok(())
    }

    async fn touch_token(&self, id: &str) -> Result<(), AuthError> {
        sqlx::query("UPDATE auth_api_tokens SET last_used_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(())
    }

    // --- Permissions ---

    async fn list_permissions(&self) -> Result<Vec<crate::types::Permission>, AuthError> {
        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT id, action, description, created_at::text FROM auth_permissions ORDER BY action",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| crate::types::Permission {
                id: r.0,
                action: r.1,
                description: r.2,
                created_at: r.3,
            })
            .collect())
    }

    async fn create_permission(
        &self,
        action: &str,
        description: &str,
    ) -> Result<crate::types::Permission, AuthError> {
        let id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO auth_permissions (id, action, description) VALUES ($1, $2, $3) ON CONFLICT (action) DO NOTHING",
        )
        .bind(&id)
        .bind(action)
        .bind(description)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        let row = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT id, action, description, created_at::text FROM auth_permissions WHERE action = $1",
        )
        .bind(action)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(crate::types::Permission {
            id: row.0,
            action: row.1,
            description: row.2,
            created_at: row.3,
        })
    }

    // --- Audit ---

    async fn log_audit(
        &self,
        user_id: Option<&str>,
        action: &str,
        target: Option<&str>,
        ip_addr: Option<&str>,
    ) -> Result<(), AuthError> {
        sqlx::query(
            "INSERT INTO auth_audit_log (user_id, action, target, ip_addr) VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(action)
        .bind(target)
        .bind(ip_addr)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_audit(&self, limit: u32, offset: u32) -> Result<Vec<AuditEntry>, AuthError> {
        let rows = sqlx::query_as::<_, (i64, Option<String>, String, Option<String>, Option<String>, String)>(
            "SELECT id, user_id, action, target, ip_addr, created_at::text
             FROM auth_audit_log ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| AuditEntry {
                id: r.0,
                user_id: r.1,
                action: r.2,
                target: r.3,
                ip_addr: r.4,
                created_at: r.5,
            })
            .collect())
    }
}
