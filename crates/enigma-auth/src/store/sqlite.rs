use std::sync::Mutex;

use async_trait::async_trait;
use rusqlite::Connection;

use super::AuthStore;
use crate::error::AuthError;
use crate::types::*;

pub struct SqliteAuthStore {
    conn: Mutex<Connection>,
}

impl SqliteAuthStore {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    pub fn open(path: &str) -> Result<Self, AuthError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self::new(conn))
    }

    pub fn open_in_memory() -> Result<Self, AuthError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(Self::new(conn))
    }
}

const MIGRATE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS auth_users (
    id TEXT PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    email TEXT UNIQUE,
    password_hash TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS auth_groups (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    is_system INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS auth_permissions (
    id TEXT PRIMARY KEY,
    action TEXT UNIQUE NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
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
    expires_at TEXT,
    last_used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS auth_audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT,
    action TEXT NOT NULL,
    target TEXT,
    ip_addr TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

#[async_trait]
impl AuthStore for SqliteAuthStore {
    async fn migrate(&self) -> Result<(), AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(MIGRATE_SQL)?;
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
        let id = {
            let conn = self.conn.lock().unwrap();
            let id = uuid::Uuid::now_v7().to_string();
            conn.execute(
                "INSERT INTO auth_users (id, username, email, password_hash) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, username, email, password_hash],
            )
            .map_err(|e| {
                if let rusqlite::Error::SqliteFailure(ref err, _) = e {
                    if err.extended_code == 2067 {
                        return AuthError::Duplicate(format!("user '{username}' already exists"));
                    }
                }
                AuthError::Database(e.to_string())
            })?;
            id
        };
        self.get_user_by_id(&id).await
    }

    async fn get_user_by_id(&self, id: &str) -> Result<User, AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, username, email, is_active, created_at, updated_at FROM auth_users WHERE id = ?1",
            [id],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    email: row.get(2)?,
                    is_active: row.get::<_, i32>(3)? != 0,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AuthError::NotFound("user not found".into()),
            _ => AuthError::Database(e.to_string()),
        })
    }

    async fn get_user_by_username(&self, username: &str) -> Result<User, AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, username, email, is_active, created_at, updated_at FROM auth_users WHERE username = ?1",
            [username],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    email: row.get(2)?,
                    is_active: row.get::<_, i32>(3)? != 0,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AuthError::NotFound("user not found".into()),
            _ => AuthError::Database(e.to_string()),
        })
    }

    async fn list_users(&self) -> Result<Vec<User>, AuthError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, email, is_active, created_at, updated_at FROM auth_users ORDER BY created_at",
        )?;
        let users = stmt
            .query_map([], |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    email: row.get(2)?,
                    is_active: row.get::<_, i32>(3)? != 0,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(users)
    }

    async fn update_user(&self, id: &str, req: &UpdateUserRequest) -> Result<User, AuthError> {
        {
            let conn = self.conn.lock().unwrap();
            if let Some(ref email) = req.email {
                conn.execute(
                    "UPDATE auth_users SET email = ?1, updated_at = datetime('now') WHERE id = ?2",
                    rusqlite::params![email, id],
                )?;
            }
            if let Some(is_active) = req.is_active {
                conn.execute(
                    "UPDATE auth_users SET is_active = ?1, updated_at = datetime('now') WHERE id = ?2",
                    rusqlite::params![is_active as i32, id],
                )?;
            }
        }
        self.get_user_by_id(id).await
    }

    async fn update_password(&self, id: &str, password_hash: &str) -> Result<(), AuthError> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE auth_users SET password_hash = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![password_hash, id],
        )?;
        if changed == 0 {
            return Err(AuthError::NotFound("user not found".into()));
        }
        Ok(())
    }

    async fn delete_user(&self, id: &str) -> Result<(), AuthError> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM auth_users WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(AuthError::NotFound("user not found".into()));
        }
        Ok(())
    }

    async fn get_password_hash(&self, user_id: &str) -> Result<String, AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT password_hash FROM auth_users WHERE id = ?1",
            [user_id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AuthError::NotFound("user not found".into()),
            _ => AuthError::Database(e.to_string()),
        })
    }

    async fn user_count(&self) -> Result<u64, AuthError> {
        let conn = self.conn.lock().unwrap();
        let count: u64 = conn.query_row("SELECT COUNT(*) FROM auth_users", [], |row| row.get(0))?;
        Ok(count)
    }

    // --- Groups ---

    async fn create_group(
        &self,
        name: &str,
        description: &str,
        is_system: bool,
    ) -> Result<Group, AuthError> {
        let id = {
            let conn = self.conn.lock().unwrap();
            let id = uuid::Uuid::now_v7().to_string();
            conn.execute(
                "INSERT INTO auth_groups (id, name, description, is_system) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, name, description, is_system as i32],
            )
            .map_err(|e| {
                if let rusqlite::Error::SqliteFailure(ref err, _) = e {
                    if err.extended_code == 2067 {
                        return AuthError::Duplicate(format!("group '{name}' already exists"));
                    }
                }
                AuthError::Database(e.to_string())
            })?;
            id
        };
        self.get_group(&id).await
    }

    async fn get_group(&self, id: &str) -> Result<Group, AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, description, is_system, created_at FROM auth_groups WHERE id = ?1",
            [id],
            |row| {
                Ok(Group {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    is_system: row.get::<_, i32>(3)? != 0,
                    created_at: row.get(4)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AuthError::NotFound("group not found".into()),
            _ => AuthError::Database(e.to_string()),
        })
    }

    async fn get_group_by_name(&self, name: &str) -> Result<Group, AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, description, is_system, created_at FROM auth_groups WHERE name = ?1",
            [name],
            |row| {
                Ok(Group {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    is_system: row.get::<_, i32>(3)? != 0,
                    created_at: row.get(4)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AuthError::NotFound("group not found".into()),
            _ => AuthError::Database(e.to_string()),
        })
    }

    async fn list_groups(&self) -> Result<Vec<Group>, AuthError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, is_system, created_at FROM auth_groups ORDER BY name",
        )?;
        let groups = stmt
            .query_map([], |row| {
                Ok(Group {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    is_system: row.get::<_, i32>(3)? != 0,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(groups)
    }

    async fn update_group(&self, id: &str, req: &UpdateGroupRequest) -> Result<Group, AuthError> {
        {
            if let Some(ref desc) = req.description {
                let conn = self.conn.lock().unwrap();
                let changed = conn.execute(
                    "UPDATE auth_groups SET description = ?1 WHERE id = ?2",
                    rusqlite::params![desc, id],
                )?;
                if changed == 0 {
                    return Err(AuthError::NotFound("group not found".into()));
                }
            }
        }
        self.get_group(id).await
    }

    async fn delete_group(&self, id: &str) -> Result<(), AuthError> {
        let conn = self.conn.lock().unwrap();
        // Check system group
        let is_system: bool = conn
            .query_row(
                "SELECT is_system FROM auth_groups WHERE id = ?1",
                [id],
                |row| row.get::<_, i32>(0),
            )
            .map(|v| v != 0)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AuthError::NotFound("group not found".into())
                }
                _ => AuthError::Database(e.to_string()),
            })?;
        if is_system {
            return Err(AuthError::Forbidden("cannot delete system group".into()));
        }
        conn.execute("DELETE FROM auth_groups WHERE id = ?1", [id])?;
        Ok(())
    }

    // --- Group-Permission ---

    async fn add_group_permission(
        &self,
        group_id: &str,
        permission_id: &str,
    ) -> Result<(), AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO auth_group_permissions (group_id, permission_id) VALUES (?1, ?2)",
            rusqlite::params![group_id, permission_id],
        )?;
        Ok(())
    }

    async fn remove_group_permission(
        &self,
        group_id: &str,
        permission_id: &str,
    ) -> Result<(), AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM auth_group_permissions WHERE group_id = ?1 AND permission_id = ?2",
            rusqlite::params![group_id, permission_id],
        )?;
        Ok(())
    }

    async fn list_group_permissions(&self, group_id: &str) -> Result<Vec<Permission>, AuthError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.id, p.action, p.description, p.created_at
             FROM auth_permissions p
             JOIN auth_group_permissions gp ON gp.permission_id = p.id
             WHERE gp.group_id = ?1
             ORDER BY p.action",
        )?;
        let perms = stmt
            .query_map([group_id], |row| {
                Ok(Permission {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(perms)
    }

    // --- User-Group ---

    async fn add_user_group(&self, user_id: &str, group_id: &str) -> Result<(), AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO auth_user_groups (user_id, group_id) VALUES (?1, ?2)",
            rusqlite::params![user_id, group_id],
        )?;
        Ok(())
    }

    async fn remove_user_group(&self, user_id: &str, group_id: &str) -> Result<(), AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM auth_user_groups WHERE user_id = ?1 AND group_id = ?2",
            rusqlite::params![user_id, group_id],
        )?;
        Ok(())
    }

    async fn list_user_groups(&self, user_id: &str) -> Result<Vec<Group>, AuthError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT g.id, g.name, g.description, g.is_system, g.created_at
             FROM auth_groups g
             JOIN auth_user_groups ug ON ug.group_id = g.id
             WHERE ug.user_id = ?1
             ORDER BY g.name",
        )?;
        let groups = stmt
            .query_map([user_id], |row| {
                Ok(Group {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    is_system: row.get::<_, i32>(3)? != 0,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(groups)
    }

    async fn get_user_permissions(&self, user_id: &str) -> Result<Vec<String>, AuthError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT p.action
             FROM auth_permissions p
             JOIN auth_group_permissions gp ON gp.permission_id = p.id
             JOIN auth_user_groups ug ON ug.group_id = gp.group_id
             WHERE ug.user_id = ?1
             ORDER BY p.action",
        )?;
        let perms = stmt
            .query_map([user_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(perms)
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
        let conn = self.conn.lock().unwrap();
        let id = uuid::Uuid::now_v7().to_string();
        conn.execute(
            "INSERT INTO auth_api_tokens (id, user_id, name, token_hash, token_prefix, scopes, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, user_id, name, token_hash, token_prefix, scopes, expires_at],
        )?;
        conn.query_row(
            "SELECT id, user_id, name, token_prefix, scopes, expires_at, last_used_at, created_at
             FROM auth_api_tokens WHERE id = ?1",
            [&id],
            |row| {
                Ok(ApiToken {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    name: row.get(2)?,
                    token_prefix: row.get(3)?,
                    scopes: row.get(4)?,
                    expires_at: row.get(5)?,
                    last_used_at: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
        .map_err(|e| AuthError::Database(e.to_string()))
    }

    async fn verify_token(&self, token_hash: &str) -> Result<(ApiToken, User), AuthError> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT t.id, t.user_id, t.name, t.token_prefix, t.scopes, t.expires_at, t.last_used_at, t.created_at,
                    u.id, u.username, u.email, u.is_active, u.created_at, u.updated_at
             FROM auth_api_tokens t
             JOIN auth_users u ON u.id = t.user_id
             WHERE t.token_hash = ?1",
            [token_hash],
            |row| {
                Ok((
                    ApiToken {
                        id: row.get(0)?,
                        user_id: row.get(1)?,
                        name: row.get(2)?,
                        token_prefix: row.get(3)?,
                        scopes: row.get(4)?,
                        expires_at: row.get(5)?,
                        last_used_at: row.get(6)?,
                        created_at: row.get(7)?,
                    },
                    User {
                        id: row.get(8)?,
                        username: row.get(9)?,
                        email: row.get(10)?,
                        is_active: row.get::<_, i32>(11)? != 0,
                        created_at: row.get(12)?,
                        updated_at: row.get(13)?,
                    },
                ))
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AuthError::Unauthorized,
            _ => AuthError::Database(e.to_string()),
        })?;

        // Check if user is active
        if !result.1.is_active {
            return Err(AuthError::Unauthorized);
        }

        // Check token expiry
        if let Some(ref exp) = result.0.expires_at {
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            if exp < &now {
                return Err(AuthError::Unauthorized);
            }
        }

        Ok(result)
    }

    async fn list_tokens(&self, user_id: &str) -> Result<Vec<ApiToken>, AuthError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, token_prefix, scopes, expires_at, last_used_at, created_at
             FROM auth_api_tokens WHERE user_id = ?1 ORDER BY created_at DESC",
        )?;
        let tokens = stmt
            .query_map([user_id], |row| {
                Ok(ApiToken {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    name: row.get(2)?,
                    token_prefix: row.get(3)?,
                    scopes: row.get(4)?,
                    expires_at: row.get(5)?,
                    last_used_at: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tokens)
    }

    async fn revoke_token(&self, id: &str) -> Result<(), AuthError> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM auth_api_tokens WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(AuthError::NotFound("token not found".into()));
        }
        Ok(())
    }

    async fn touch_token(&self, id: &str) -> Result<(), AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE auth_api_tokens SET last_used_at = datetime('now') WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    // --- Permissions ---

    async fn list_permissions(&self) -> Result<Vec<Permission>, AuthError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, action, description, created_at FROM auth_permissions ORDER BY action",
        )?;
        let perms = stmt
            .query_map([], |row| {
                Ok(Permission {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(perms)
    }

    async fn create_permission(
        &self,
        action: &str,
        description: &str,
    ) -> Result<Permission, AuthError> {
        let conn = self.conn.lock().unwrap();
        let id = uuid::Uuid::now_v7().to_string();
        conn.execute(
            "INSERT OR IGNORE INTO auth_permissions (id, action, description) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, action, description],
        )?;
        // Return existing or new
        conn.query_row(
            "SELECT id, action, description, created_at FROM auth_permissions WHERE action = ?1",
            [action],
            |row| {
                Ok(Permission {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )
        .map_err(|e| AuthError::Database(e.to_string()))
    }

    // --- Audit ---

    async fn log_audit(
        &self,
        user_id: Option<&str>,
        action: &str,
        target: Option<&str>,
        ip_addr: Option<&str>,
    ) -> Result<(), AuthError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO auth_audit_log (user_id, action, target, ip_addr) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![user_id, action, target, ip_addr],
        )?;
        Ok(())
    }

    async fn list_audit(&self, limit: u32, offset: u32) -> Result<Vec<AuditEntry>, AuthError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, action, target, ip_addr, created_at
             FROM auth_audit_log ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
        )?;
        let entries = stmt
            .query_map(rusqlite::params![limit, offset], |row| {
                Ok(AuditEntry {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    action: row.get(2)?,
                    target: row.get(3)?,
                    ip_addr: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }
}
