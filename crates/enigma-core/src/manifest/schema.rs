use rusqlite::Connection;

use crate::error::Result;

/// Run all migrations on the database.
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode=WAL;
        PRAGMA foreign_keys=ON;

        CREATE TABLE IF NOT EXISTS providers (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL UNIQUE,
            type        TEXT NOT NULL,
            bucket      TEXT NOT NULL,
            region      TEXT,
            weight      INTEGER NOT NULL DEFAULT 1,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS backups (
            id              TEXT PRIMARY KEY,
            source_path     TEXT NOT NULL,
            status          TEXT NOT NULL DEFAULT 'in_progress',
            total_files     INTEGER NOT NULL DEFAULT 0,
            total_bytes     INTEGER NOT NULL DEFAULT 0,
            total_chunks    INTEGER NOT NULL DEFAULT 0,
            dedup_chunks    INTEGER NOT NULL DEFAULT 0,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            completed_at    TEXT
        );

        CREATE TABLE IF NOT EXISTS backup_files (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            backup_id       TEXT NOT NULL REFERENCES backups(id),
            path            TEXT NOT NULL,
            size            INTEGER NOT NULL,
            mtime           TEXT,
            hash            TEXT NOT NULL,
            chunk_count     INTEGER NOT NULL DEFAULT 0,
            UNIQUE(backup_id, path)
        );

        CREATE TABLE IF NOT EXISTS chunks (
            hash            TEXT PRIMARY KEY,
            nonce           BLOB NOT NULL,
            key_id          TEXT NOT NULL,
            provider_id     INTEGER NOT NULL REFERENCES providers(id),
            storage_key     TEXT NOT NULL,
            size_plain      INTEGER NOT NULL,
            size_encrypted  INTEGER NOT NULL,
            ref_count       INTEGER NOT NULL DEFAULT 1,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS file_chunks (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            file_id         INTEGER NOT NULL REFERENCES backup_files(id),
            chunk_hash      TEXT NOT NULL REFERENCES chunks(hash),
            chunk_index     INTEGER NOT NULL,
            offset          INTEGER NOT NULL,
            UNIQUE(file_id, chunk_index)
        );

        CREATE TABLE IF NOT EXISTS backup_logs (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            backup_id   TEXT REFERENCES backups(id),
            level       TEXT NOT NULL,
            message     TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- S3 gateway tables
        CREATE TABLE IF NOT EXISTS namespaces (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL UNIQUE,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS objects (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            namespace_id    INTEGER NOT NULL REFERENCES namespaces(id),
            key             TEXT NOT NULL,
            size            INTEGER NOT NULL,
            etag            TEXT NOT NULL,
            content_type    TEXT,
            chunk_count     INTEGER NOT NULL,
            key_id          TEXT NOT NULL,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(namespace_id, key)
        );

        CREATE TABLE IF NOT EXISTS object_chunks (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            object_id       INTEGER NOT NULL REFERENCES objects(id),
            chunk_hash      TEXT NOT NULL,
            chunk_index     INTEGER NOT NULL,
            offset          INTEGER NOT NULL,
            UNIQUE(object_id, chunk_index)
        );

        CREATE TABLE IF NOT EXISTS multipart_uploads (
            id              TEXT PRIMARY KEY,
            namespace_id    INTEGER NOT NULL REFERENCES namespaces(id),
            key             TEXT NOT NULL,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS multipart_parts (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            upload_id       TEXT NOT NULL REFERENCES multipart_uploads(id),
            part_number     INTEGER NOT NULL,
            data            BLOB NOT NULL,
            size            INTEGER NOT NULL,
            etag            TEXT NOT NULL,
            UNIQUE(upload_id, part_number)
        );
        ",
    )?;

    // v2 migration: add size_compressed column (NULL = not compressed).
    // Ignore "duplicate column name" error for idempotency.
    let _ = conn.execute("ALTER TABLE chunks ADD COLUMN size_compressed INTEGER", []);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"providers".to_string()));
        assert!(tables.contains(&"backups".to_string()));
        assert!(tables.contains(&"chunks".to_string()));
        assert!(tables.contains(&"file_chunks".to_string()));
        assert!(tables.contains(&"backup_files".to_string()));
        assert!(tables.contains(&"backup_logs".to_string()));
        assert!(tables.contains(&"namespaces".to_string()));
        assert!(tables.contains(&"objects".to_string()));
        assert!(tables.contains(&"object_chunks".to_string()));
        assert!(tables.contains(&"multipart_uploads".to_string()));
        assert!(tables.contains(&"multipart_parts".to_string()));
    }

    #[test]
    fn migrate_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap(); // Should not fail
    }
}
