use rusqlite::{Connection, params};
use std::path::Path;
use std::time::Duration;

use crate::error::{EnigmaError, Result};
use crate::types::{BackupRecord, BackupStatus, ProviderInfo, ProviderType};

/// High-level interface for manifest database operations.
pub struct ManifestDb {
    conn: Connection,
}

impl ManifestDb {
    /// Open (or create) the manifest database and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        super::schema::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        super::schema::migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // ── Providers ──────────────────────────────────────────────

    pub fn insert_provider(
        &self,
        name: &str,
        provider_type: ProviderType,
        bucket: &str,
        region: Option<&str>,
        weight: u32,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO providers (name, type, bucket, region, weight) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, provider_type.to_string(), bucket, region, weight],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_providers(&self) -> Result<Vec<ProviderInfo>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, type, bucket, region, weight FROM providers")?;
        let rows = stmt.query_map([], |row| {
            Ok(ProviderInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                provider_type: row
                    .get::<_, String>(2)?
                    .parse()
                    .unwrap_or(ProviderType::Local),
                bucket: row.get(3)?,
                region: row.get(4)?,
                weight: row.get(5)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    // ── Backups ────────────────────────────────────────────────

    pub fn create_backup(&self, id: &str, source_path: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO backups (id, source_path) VALUES (?1, ?2)",
            params![id, source_path],
        )?;
        Ok(())
    }

    pub fn complete_backup(
        &self,
        id: &str,
        total_files: u64,
        total_bytes: u64,
        total_chunks: u64,
        dedup_chunks: u64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE backups SET status='completed', total_files=?2, total_bytes=?3, total_chunks=?4, dedup_chunks=?5, completed_at=datetime('now') WHERE id=?1",
            params![id, total_files, total_bytes, total_chunks, dedup_chunks],
        )?;
        Ok(())
    }

    pub fn fail_backup(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE backups SET status='failed', completed_at=datetime('now') WHERE id=?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn get_backup(&self, id: &str) -> Result<BackupRecord> {
        self.conn
            .query_row(
                "SELECT id, source_path, status, total_files, total_bytes, total_chunks, dedup_chunks, created_at, completed_at FROM backups WHERE id=?1",
                params![id],
                |row| {
                    Ok(BackupRecord {
                        id: row.get(0)?,
                        source_path: row.get(1)?,
                        status: row
                            .get::<_, String>(2)?
                            .parse()
                            .unwrap_or(BackupStatus::Failed),
                        total_files: row.get(3)?,
                        total_bytes: row.get(4)?,
                        total_chunks: row.get(5)?,
                        dedup_chunks: row.get(6)?,
                        created_at: row.get(7)?,
                        completed_at: row.get(8)?,
                    })
                },
            )
            .map_err(|_| EnigmaError::BackupNotFound(id.to_string()))
    }

    pub fn list_backups(&self) -> Result<Vec<BackupRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_path, status, total_files, total_bytes, total_chunks, dedup_chunks, created_at, completed_at FROM backups ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(BackupRecord {
                id: row.get(0)?,
                source_path: row.get(1)?,
                status: row
                    .get::<_, String>(2)?
                    .parse()
                    .unwrap_or(BackupStatus::Failed),
                total_files: row.get(3)?,
                total_bytes: row.get(4)?,
                total_chunks: row.get(5)?,
                dedup_chunks: row.get(6)?,
                created_at: row.get(7)?,
                completed_at: row.get(8)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn latest_backup(&self) -> Result<Option<BackupRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_path, status, total_files, total_bytes, total_chunks, dedup_chunks, created_at, completed_at FROM backups ORDER BY created_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], |row| {
            Ok(BackupRecord {
                id: row.get(0)?,
                source_path: row.get(1)?,
                status: row
                    .get::<_, String>(2)?
                    .parse()
                    .unwrap_or(BackupStatus::Failed),
                total_files: row.get(3)?,
                total_bytes: row.get(4)?,
                total_chunks: row.get(5)?,
                dedup_chunks: row.get(6)?,
                created_at: row.get(7)?,
                completed_at: row.get(8)?,
            })
        })?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    // ── Backup files ───────────────────────────────────────────

    pub fn insert_backup_file(
        &self,
        backup_id: &str,
        path: &str,
        size: u64,
        mtime: Option<&str>,
        hash: &str,
        chunk_count: u32,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO backup_files (backup_id, path, size, mtime, hash, chunk_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![backup_id, path, size, mtime, hash, chunk_count],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_backup_files(&self, backup_id: &str) -> Result<Vec<(i64, String, u64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, size, hash FROM backup_files WHERE backup_id=?1 ORDER BY path",
        )?;
        let rows = stmt.query_map(params![backup_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    // ── Chunks ─────────────────────────────────────────────────

    /// Insert a new chunk. Returns true if inserted (new), false if already existed (dedup).
    pub fn insert_or_dedup_chunk(
        &self,
        hash: &str,
        nonce: &[u8],
        key_id: &str,
        provider_id: i64,
        storage_key: &str,
        size_plain: u64,
        size_encrypted: u64,
        size_compressed: Option<u64>,
    ) -> Result<bool> {
        // Try to increment ref_count if it already exists
        let updated = self.conn.execute(
            "UPDATE chunks SET ref_count = ref_count + 1 WHERE hash = ?1",
            params![hash],
        )?;

        if updated > 0 {
            return Ok(false); // deduped
        }

        self.conn.execute(
            "INSERT INTO chunks (hash, nonce, key_id, provider_id, storage_key, size_plain, size_encrypted, size_compressed) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![hash, nonce, key_id, provider_id, storage_key, size_plain, size_encrypted, size_compressed],
        )?;
        Ok(true) // new
    }

    pub fn get_chunk_info(
        &self,
        hash: &str,
    ) -> Result<Option<(Vec<u8>, String, i64, String, u64, Option<u64>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT nonce, key_id, provider_id, storage_key, size_encrypted, size_compressed FROM chunks WHERE hash=?1",
        )?;
        let mut rows = stmt.query_map(params![hash], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, u64>(4)?,
                row.get::<_, Option<u64>>(5)?,
            ))
        })?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    /// Decrement ref_count. If it reaches 0, return ALL storage locations for deletion
    /// (primary + replicas). The ON DELETE CASCADE cleans up chunk_replicas automatically.
    pub fn decrement_chunk_ref(&self, hash: &str) -> Result<Vec<(i64, String)>> {
        self.conn.execute(
            "UPDATE chunks SET ref_count = ref_count - 1 WHERE hash = ?1",
            params![hash],
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT provider_id, storage_key FROM chunks WHERE hash=?1 AND ref_count <= 0",
        )?;
        let mut rows = stmt.query_map(params![hash], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;

        if let Some(Ok(primary)) = rows.next() {
            // Collect all replica locations before deleting
            let replicas = self.get_chunk_replicas(hash)?;
            let mut all_locations = vec![primary];
            for (pid, skey) in replicas {
                // Avoid duplicating the primary
                if !all_locations.iter().any(|(id, _)| *id == pid) {
                    all_locations.push((pid, skey));
                }
            }
            // Delete the chunk record (cascades to chunk_replicas)
            self.conn
                .execute("DELETE FROM chunks WHERE hash=?1", params![hash])?;
            Ok(all_locations)
        } else {
            Ok(vec![])
        }
    }

    // ── Chunk Replicas ──────────────────────────────────────────

    /// Insert replica records for a chunk (called when replication_factor > 1).
    pub fn insert_chunk_replicas(&self, chunk_hash: &str, replicas: &[(i64, &str)]) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "INSERT OR IGNORE INTO chunk_replicas (chunk_hash, provider_id, storage_key) VALUES (?1, ?2, ?3)",
        )?;
        for (provider_id, storage_key) in replicas {
            stmt.execute(params![chunk_hash, provider_id, storage_key])?;
        }
        Ok(())
    }

    /// Get all replica locations for a chunk.
    pub fn get_chunk_replicas(&self, chunk_hash: &str) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider_id, storage_key FROM chunk_replicas WHERE chunk_hash=?1",
        )?;
        let rows = stmt.query_map(params![chunk_hash], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Get all storage locations for a chunk (replicas first, then legacy fallback).
    /// Returns: (nonce, key_id, Vec<(provider_id, storage_key)>, size_encrypted, size_compressed)
    pub fn get_chunk_locations(
        &self,
        hash: &str,
    ) -> Result<Option<(Vec<u8>, String, Vec<(i64, String)>, u64, Option<u64>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT nonce, key_id, provider_id, storage_key, size_encrypted, size_compressed FROM chunks WHERE hash=?1",
        )?;
        let mut rows = stmt.query_map(params![hash], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, u64>(4)?,
                row.get::<_, Option<u64>>(5)?,
            ))
        })?;

        let Some(Ok((nonce, key_id, primary_pid, primary_skey, size_enc, size_compressed))) =
            rows.next()
        else {
            return Ok(None);
        };

        // Try replicas first
        let replicas = self.get_chunk_replicas(hash)?;
        let locations = if replicas.is_empty() {
            // Legacy fallback: use the primary provider_id from chunks table
            vec![(primary_pid, primary_skey)]
        } else {
            replicas
        };

        Ok(Some((nonce, key_id, locations, size_enc, size_compressed)))
    }

    /// Find orphan chunk replicas (replicas whose parent chunk has ref_count <= 0
    /// or doesn't exist). Returns (chunk_hash, provider_id, storage_key).
    pub fn find_orphan_chunk_replicas(&self) -> Result<Vec<(String, i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT cr.chunk_hash, cr.provider_id, cr.storage_key FROM chunk_replicas cr
             LEFT JOIN chunks c ON cr.chunk_hash = c.hash
             WHERE c.hash IS NULL OR c.ref_count <= 0",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    // ── File-chunk mapping ─────────────────────────────────────

    pub fn insert_file_chunk(
        &self,
        file_id: i64,
        chunk_hash: &str,
        chunk_index: u32,
        offset: u64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO file_chunks (file_id, chunk_hash, chunk_index, offset) VALUES (?1, ?2, ?3, ?4)",
            params![file_id, chunk_hash, chunk_index, offset],
        )?;
        Ok(())
    }

    pub fn get_file_chunks(&self, file_id: i64) -> Result<Vec<(String, u32, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT chunk_hash, chunk_index, offset FROM file_chunks WHERE file_id=?1 ORDER BY chunk_index",
        )?;
        let rows = stmt.query_map(params![file_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    // ── Logs ───────────────────────────────────────────────────

    pub fn log(&self, backup_id: Option<&str>, level: &str, message: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO backup_logs (backup_id, level, message) VALUES (?1, ?2, ?3)",
            params![backup_id, level, message],
        )?;
        Ok(())
    }

    pub fn get_logs(&self, backup_id: &str) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT level, message, created_at FROM backup_logs WHERE backup_id=?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![backup_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    // ── GC (Garbage Collection) ──────────────────────────────

    /// Find orphaned chunks: chunks with ref_count <= 0 that are not referenced
    /// by any file_chunks or object_chunks.
    pub fn find_orphan_chunks(&self) -> Result<Vec<(String, i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.hash, c.provider_id, c.storage_key FROM chunks c
             WHERE c.ref_count <= 0
             AND NOT EXISTS (SELECT 1 FROM file_chunks fc WHERE fc.chunk_hash = c.hash)
             AND NOT EXISTS (SELECT 1 FROM object_chunks oc WHERE oc.chunk_hash = c.hash)",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Delete a chunk record by hash.
    pub fn delete_chunk_record(&self, hash: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM chunks WHERE hash = ?1", params![hash])?;
        Ok(())
    }

    /// Get chunk stats: (total_chunks, orphan_chunks).
    pub fn chunk_stats(&self) -> Result<(u64, u64)> {
        let total: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))?;
        let orphans: u64 = self.conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE ref_count <= 0
             AND NOT EXISTS (SELECT 1 FROM file_chunks fc WHERE fc.chunk_hash = chunks.hash)
             AND NOT EXISTS (SELECT 1 FROM object_chunks oc WHERE oc.chunk_hash = chunks.hash)",
            [],
            |row| row.get(0),
        )?;
        Ok((total, orphans))
    }

    /// Detailed chunk storage metrics.
    pub fn chunk_storage_details(
        &self,
    ) -> Result<(u64, u64, u64, Option<u64>, u64)> {
        // total_size_plain, total_size_encrypted, total_size_compressed, total_refs
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(size_plain),0), COALESCE(SUM(size_encrypted),0), SUM(size_compressed), COALESCE(SUM(ref_count),0) FROM chunks",
        )?;
        let row = stmt.query_row([], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, Option<u64>>(2)?,
                row.get::<_, u64>(3)?,
            ))
        })?;
        let total_chunks: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
        Ok((row.0, row.1, total_chunks, row.2, row.3))
    }

    /// Chunks per provider: Vec<(provider_id, provider_name, chunk_count, total_size_encrypted)>
    /// Counts both primary chunks and replicas.
    pub fn chunks_per_provider(&self) -> Result<Vec<(i64, String, u64, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider_id, provider_name, SUM(cnt), SUM(total_size) FROM (
                SELECT c.provider_id, COALESCE(p.name, 'unknown') AS provider_name, COUNT(*) AS cnt, COALESCE(SUM(c.size_encrypted),0) AS total_size
                FROM chunks c LEFT JOIN providers p ON c.provider_id = p.id
                GROUP BY c.provider_id
              UNION ALL
                SELECT cr.provider_id, COALESCE(p.name, 'unknown') AS provider_name, COUNT(*) AS cnt, COALESCE(SUM(c.size_encrypted),0) AS total_size
                FROM chunk_replicas cr
                LEFT JOIN providers p ON cr.provider_id = p.id
                LEFT JOIN chunks c ON cr.chunk_hash = c.hash
                GROUP BY cr.provider_id
             ) GROUP BY provider_id ORDER BY SUM(cnt) DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, u64>(3)?,
            ))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Recent chunks: Vec<(hash, provider_name, size_plain, size_encrypted, ref_count, created_at)>
    pub fn recent_chunks(&self, limit: u32) -> Result<Vec<(String, String, u64, u64, u64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.hash, COALESCE(p.name, 'unknown'), c.size_plain, c.size_encrypted, c.ref_count, c.created_at
             FROM chunks c LEFT JOIN providers p ON c.provider_id = p.id
             ORDER BY c.created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, u64>(3)?,
                row.get::<_, u64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    // ── Snapshots ──────────────────────────────────────────────

    /// Serialize the entire DB to bytes via the SQLite backup API.
    pub fn snapshot_to_bytes(&self) -> Result<Vec<u8>> {
        let tmp = tempfile::NamedTempFile::new()?;
        let mut dest = Connection::open(tmp.path())?;
        let backup = rusqlite::backup::Backup::new(&self.conn, &mut dest)?;
        backup.run_to_completion(100, Duration::ZERO, None)?;
        drop(backup);
        drop(dest);
        Ok(std::fs::read(tmp.path())?)
    }

    /// Restore DB from raw bytes, writing to the given path.
    pub fn restore_from_bytes(data: &[u8], path: &Path) -> Result<Self> {
        let tmp_path = path.with_extension("snap.tmp");
        std::fs::write(&tmp_path, data)?;
        // Verify it opens correctly
        let db = Self::open(&tmp_path)?;
        drop(db);
        std::fs::rename(&tmp_path, path)?;
        Self::open(path)
    }

    // ── S3 Gateway: Namespaces ───────────────────────────────

    pub fn create_namespace(&self, name: &str) -> Result<i64> {
        self.conn
            .execute("INSERT INTO namespaces (name) VALUES (?1)", params![name])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_namespace_id(&self, name: &str) -> Result<Option<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM namespaces WHERE name=?1")?;
        let mut rows = stmt.query_map(params![name], |row| row.get::<_, i64>(0))?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn namespace_exists(&self, name: &str) -> Result<bool> {
        Ok(self.get_namespace_id(name)?.is_some())
    }

    pub fn delete_namespace(&self, name: &str) -> Result<bool> {
        let deleted = self
            .conn
            .execute("DELETE FROM namespaces WHERE name=?1", params![name])?;
        Ok(deleted > 0)
    }

    pub fn list_namespaces(&self) -> Result<Vec<(i64, String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at FROM namespaces ORDER BY name")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    // ── S3 Gateway: Objects ──────────────────────────────────

    pub fn insert_object(
        &self,
        namespace_id: i64,
        key: &str,
        size: u64,
        etag: &str,
        content_type: Option<&str>,
        chunk_count: u32,
        key_id: &str,
    ) -> Result<i64> {
        // Upsert: delete old object if exists, then insert new one
        self.delete_object_by_ns_key(namespace_id, key)?;
        self.conn.execute(
            "INSERT INTO objects (namespace_id, key, size, etag, content_type, chunk_count, key_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![namespace_id, key, size, etag, content_type, chunk_count, key_id],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_object(
        &self,
        namespace_id: i64,
        key: &str,
    ) -> Result<Option<(i64, u64, String, Option<String>, u32, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, size, etag, content_type, chunk_count, key_id, created_at FROM objects WHERE namespace_id=?1 AND key=?2",
        )?;
        let mut rows = stmt.query_map(params![namespace_id, key], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, u32>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    /// Delete an object by namespace_id + key. Also cleans up object_chunks
    /// and decrements chunk ref_counts.
    /// Returns the list of (provider_id, storage_key) for chunks that need physical deletion.
    pub fn delete_object_by_ns_key(
        &self,
        namespace_id: i64,
        key: &str,
    ) -> Result<Vec<(i64, String)>> {
        let obj = self.get_object(namespace_id, key)?;
        let Some((object_id, ..)) = obj else {
            return Ok(vec![]);
        };

        // Get all chunk hashes for this object
        let chunk_hashes = self.get_object_chunks(object_id)?;
        let mut to_delete = Vec::new();

        for (chunk_hash, _, _) in &chunk_hashes {
            let locations = self.decrement_chunk_ref(chunk_hash)?;
            to_delete.extend(locations);
        }

        // Delete object_chunks and object
        self.conn.execute(
            "DELETE FROM object_chunks WHERE object_id=?1",
            params![object_id],
        )?;
        self.conn
            .execute("DELETE FROM objects WHERE id=?1", params![object_id])?;

        Ok(to_delete)
    }

    pub fn list_objects(
        &self,
        namespace_id: i64,
        prefix: &str,
        max_keys: u32,
        start_after: &str,
    ) -> Result<Vec<(String, u64, String, String)>> {
        let prefix_pattern = format!("{prefix}%");
        let mut stmt = self.conn.prepare(
            "SELECT key, size, etag, created_at FROM objects WHERE namespace_id=?1 AND key LIKE ?2 AND key > ?3 ORDER BY key LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![namespace_id, prefix_pattern, start_after, max_keys],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn count_objects_with_prefix(&self, namespace_id: i64, prefix: &str) -> Result<u64> {
        let prefix_pattern = format!("{prefix}%");
        let count: u64 = self.conn.query_row(
            "SELECT COUNT(*) FROM objects WHERE namespace_id=?1 AND key LIKE ?2",
            params![namespace_id, prefix_pattern],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    // ── S3 Gateway: Object Chunks ────────────────────────────

    pub fn insert_object_chunk(
        &self,
        object_id: i64,
        chunk_hash: &str,
        chunk_index: u32,
        offset: u64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO object_chunks (object_id, chunk_hash, chunk_index, offset) VALUES (?1, ?2, ?3, ?4)",
            params![object_id, chunk_hash, chunk_index, offset],
        )?;
        Ok(())
    }

    pub fn get_object_chunks(&self, object_id: i64) -> Result<Vec<(String, u32, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT chunk_hash, chunk_index, offset FROM object_chunks WHERE object_id=?1 ORDER BY chunk_index",
        )?;
        let rows = stmt.query_map(params![object_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    // ── S3 Gateway: Multipart uploads ────────────────────────

    pub fn create_multipart_upload(
        &self,
        upload_id: &str,
        namespace_id: i64,
        key: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO multipart_uploads (id, namespace_id, key) VALUES (?1, ?2, ?3)",
            params![upload_id, namespace_id, key],
        )?;
        Ok(())
    }

    pub fn get_multipart_upload(&self, upload_id: &str) -> Result<Option<(i64, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT namespace_id, key FROM multipart_uploads WHERE id=?1")?;
        let mut rows = stmt.query_map(params![upload_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn insert_multipart_part(
        &self,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
        etag: &str,
    ) -> Result<()> {
        // Upsert: replace if same part_number exists
        self.conn.execute(
            "INSERT OR REPLACE INTO multipart_parts (upload_id, part_number, data, size, etag) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![upload_id, part_number, data, data.len() as u64, etag],
        )?;
        Ok(())
    }

    pub fn get_multipart_parts(&self, upload_id: &str) -> Result<Vec<(i32, Vec<u8>, u64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT part_number, data, size, etag FROM multipart_parts WHERE upload_id=?1 ORDER BY part_number",
        )?;
        let rows = stmt.query_map(params![upload_id], |row| {
            Ok((
                row.get::<_, i32>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn abort_multipart_upload(&self, upload_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM multipart_parts WHERE upload_id=?1",
            params![upload_id],
        )?;
        self.conn.execute(
            "DELETE FROM multipart_uploads WHERE id=?1",
            params![upload_id],
        )?;
        Ok(())
    }

    pub fn list_multipart_uploads(
        &self,
        namespace_id: i64,
    ) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, key, created_at FROM multipart_uploads WHERE namespace_id=?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![namespace_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_backup_flow() {
        let db = ManifestDb::open_in_memory().unwrap();

        // Add provider
        let pid = db
            .insert_provider("local-test", ProviderType::Local, "/tmp/enigma", None, 1)
            .unwrap();

        // Create backup
        let backup_id = "01234567-abcd-7000-0000-000000000001";
        db.create_backup(backup_id, "/home/user/docs").unwrap();

        // Insert file
        let file_id = db
            .insert_backup_file(backup_id, "docs/readme.md", 1024, None, "abc123", 1)
            .unwrap();

        // Insert chunk (new)
        let is_new = db
            .insert_or_dedup_chunk(
                "deadbeef",
                &[0u8; 12],
                "key-1",
                pid,
                "enigma/chunks/de/ad/deadbeef",
                1024,
                1040,
                None,
            )
            .unwrap();
        assert!(is_new);

        // Insert same chunk (dedup)
        let is_new = db
            .insert_or_dedup_chunk(
                "deadbeef",
                &[0u8; 12],
                "key-1",
                pid,
                "enigma/chunks/de/ad/deadbeef",
                1024,
                1040,
                None,
            )
            .unwrap();
        assert!(!is_new);

        // Map file → chunk
        db.insert_file_chunk(file_id, "deadbeef", 0, 0).unwrap();

        // Complete backup
        db.complete_backup(backup_id, 1, 1024, 1, 1).unwrap();

        // Verify
        let backup = db.get_backup(backup_id).unwrap();
        assert_eq!(backup.status, BackupStatus::Completed);
        assert_eq!(backup.total_files, 1);

        let files = db.list_backup_files(backup_id).unwrap();
        assert_eq!(files.len(), 1);

        let chunks = db.get_file_chunks(file_id).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, "deadbeef");
    }

    #[test]
    fn list_backups_ordered() {
        let db = ManifestDb::open_in_memory().unwrap();
        db.create_backup("id-1", "/path1").unwrap();
        db.create_backup("id-2", "/path2").unwrap();

        let backups = db.list_backups().unwrap();
        assert_eq!(backups.len(), 2);
    }

    #[test]
    fn chunk_dedup_ref_counting() {
        let db = ManifestDb::open_in_memory().unwrap();
        let pid = db
            .insert_provider("test", ProviderType::Local, "/tmp", None, 1)
            .unwrap();

        // Insert chunk twice (ref_count = 2)
        db.insert_or_dedup_chunk("aaa", &[0; 12], "k1", pid, "key", 100, 116, None)
            .unwrap();
        db.insert_or_dedup_chunk("aaa", &[0; 12], "k1", pid, "key", 100, 116, None)
            .unwrap();

        // First decrement: ref_count goes to 1 — no deletion
        let result = db.decrement_chunk_ref("aaa").unwrap();
        assert!(result.is_empty());

        // Second decrement: ref_count goes to 0 — returns deletion info
        let result = db.decrement_chunk_ref("aaa").unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn log_entries() {
        let db = ManifestDb::open_in_memory().unwrap();
        db.create_backup("b1", "/path").unwrap();
        db.log(Some("b1"), "INFO", "Backup started").unwrap();
        db.log(Some("b1"), "INFO", "Backup completed").unwrap();

        let logs = db.get_logs("b1").unwrap();
        assert_eq!(logs.len(), 2);
    }

    #[test]
    fn insert_and_get_chunk_replicas() {
        let db = ManifestDb::open_in_memory().unwrap();
        let p1 = db
            .insert_provider("p1", ProviderType::Local, "/a", None, 1)
            .unwrap();
        let p2 = db
            .insert_provider("p2", ProviderType::Local, "/b", None, 1)
            .unwrap();

        db.insert_or_dedup_chunk("hash1", &[0; 12], "k1", p1, "key1", 100, 116, None)
            .unwrap();

        db.insert_chunk_replicas("hash1", &[(p1, "key1"), (p2, "key1")])
            .unwrap();

        let replicas = db.get_chunk_replicas("hash1").unwrap();
        assert_eq!(replicas.len(), 2);
        let pids: Vec<i64> = replicas.iter().map(|(pid, _)| *pid).collect();
        assert!(pids.contains(&p1));
        assert!(pids.contains(&p2));
    }

    #[test]
    fn get_chunk_locations_with_replicas() {
        let db = ManifestDb::open_in_memory().unwrap();
        let p1 = db
            .insert_provider("p1", ProviderType::Local, "/a", None, 1)
            .unwrap();
        let p2 = db
            .insert_provider("p2", ProviderType::Local, "/b", None, 1)
            .unwrap();

        db.insert_or_dedup_chunk("hash2", &[1; 12], "k1", p1, "skey", 200, 216, None)
            .unwrap();
        db.insert_chunk_replicas("hash2", &[(p1, "skey"), (p2, "skey")])
            .unwrap();

        let loc = db.get_chunk_locations("hash2").unwrap().unwrap();
        let (_nonce, _key_id, locations, _size_enc, _size_comp) = loc;
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn get_chunk_locations_legacy_fallback() {
        let db = ManifestDb::open_in_memory().unwrap();
        let p1 = db
            .insert_provider("p1", ProviderType::Local, "/a", None, 1)
            .unwrap();

        db.insert_or_dedup_chunk("hash3", &[2; 12], "k1", p1, "skey3", 300, 316, None)
            .unwrap();
        // No replicas inserted — should fallback to primary from chunks table

        let loc = db.get_chunk_locations("hash3").unwrap().unwrap();
        let (_nonce, _key_id, locations, _size_enc, _size_comp) = loc;
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].0, p1);
        assert_eq!(locations[0].1, "skey3");
    }

    #[test]
    fn decrement_chunk_ref_returns_all_replicas() {
        let db = ManifestDb::open_in_memory().unwrap();
        let p1 = db
            .insert_provider("p1", ProviderType::Local, "/a", None, 1)
            .unwrap();
        let p2 = db
            .insert_provider("p2", ProviderType::Local, "/b", None, 1)
            .unwrap();

        db.insert_or_dedup_chunk("hash4", &[3; 12], "k1", p1, "skey4", 400, 416, None)
            .unwrap();
        db.insert_chunk_replicas("hash4", &[(p1, "skey4"), (p2, "skey4")])
            .unwrap();

        // Decrement to 0 → should return both provider locations
        let result = db.decrement_chunk_ref("hash4").unwrap();
        assert_eq!(result.len(), 2);
        let pids: Vec<i64> = result.iter().map(|(pid, _)| *pid).collect();
        assert!(pids.contains(&p1));
        assert!(pids.contains(&p2));

        // Chunk should be deleted, replicas cascaded
        let replicas = db.get_chunk_replicas("hash4").unwrap();
        assert!(replicas.is_empty());
    }

    #[test]
    fn cascade_delete_chunk_replicas() {
        let db = ManifestDb::open_in_memory().unwrap();
        let p1 = db
            .insert_provider("p1", ProviderType::Local, "/a", None, 1)
            .unwrap();
        let p2 = db
            .insert_provider("p2", ProviderType::Local, "/b", None, 1)
            .unwrap();

        db.insert_or_dedup_chunk("hash5", &[4; 12], "k1", p1, "skey5", 500, 516, None)
            .unwrap();
        db.insert_chunk_replicas("hash5", &[(p1, "skey5"), (p2, "skey5")])
            .unwrap();

        // Direct delete of chunk record should cascade to replicas
        db.delete_chunk_record("hash5").unwrap();

        let replicas = db.get_chunk_replicas("hash5").unwrap();
        assert!(replicas.is_empty());
    }
}
