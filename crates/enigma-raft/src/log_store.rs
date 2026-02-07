use std::sync::Mutex;

use openraft::storage::{LogFlushed, RaftLogStorage};
use openraft::{Entry, LogId, LogState, OptionalSend, StorageError, StorageIOError, Vote};

use crate::TypeConfig;

/// SQLite-backed Raft log storage.
pub struct SqliteLogStore {
    conn: Mutex<rusqlite::Connection>,
    path: Option<String>,
}

impl SqliteLogStore {
    pub fn new(path: &str) -> anyhow::Result<Self> {
        std::fs::create_dir_all(
            std::path::Path::new(path)
                .parent()
                .unwrap_or(std::path::Path::new(".")),
        )?;

        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA foreign_keys=ON;

            CREATE TABLE IF NOT EXISTS raft_log (
                log_index INTEGER PRIMARY KEY,
                term INTEGER NOT NULL,
                entry BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS raft_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: Some(path.to_string()),
        })
    }

    /// Create an in-memory log store (for testing / single-node).
    pub fn in_memory() -> anyhow::Result<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS raft_log (
                log_index INTEGER PRIMARY KEY,
                term INTEGER NOT NULL,
                entry BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS raft_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: None,
        })
    }

    fn get_state_value(&self, key: &str) -> Result<Option<String>, StorageError<u64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT value FROM raft_state WHERE key=?1")
            .map_err(|e| StorageError::IO {
                source: StorageIOError::read(&e),
            })?;
        let mut rows = stmt
            .query_map(rusqlite::params![key], |row| row.get::<_, String>(0))
            .map_err(|e| StorageError::IO {
                source: StorageIOError::read(&e),
            })?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    fn set_state_value(&self, key: &str, value: &str) -> Result<(), StorageError<u64>> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO raft_state (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )
        .map_err(|e| StorageError::IO {
            source: StorageIOError::write(&e),
        })?;
        Ok(())
    }
}

impl RaftLogStorage<TypeConfig> for SqliteLogStore {
    type LogReader = Self;

    async fn get_log_state(&mut self) -> Result<LogState<TypeConfig>, StorageError<u64>> {
        let conn = self.conn.lock().unwrap();

        // Get last log entry
        let last: Option<(u64, u64)> = conn
            .query_row(
                "SELECT log_index, term FROM raft_log ORDER BY log_index DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let last_log_id =
            last.map(|(index, term)| LogId::new(openraft::CommittedLeaderId::new(term, 0), index));

        // Get last purged (stored in raft_state)
        drop(conn);
        let last_purged = self
            .get_state_value("last_purged")?
            .and_then(|v| serde_json::from_str(&v).ok());

        Ok(LogState {
            last_purged_log_id: last_purged,
            last_log_id,
        })
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        // Open a separate read-only connection to the same DB
        match &self.path {
            Some(path) => {
                let conn = rusqlite::Connection::open_with_flags(
                    path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )
                .expect("Failed to open log reader connection");
                Self {
                    conn: Mutex::new(conn),
                    path: Some(path.clone()),
                }
            }
            None => {
                // In-memory: can't share, create a new empty one (shouldn't be used in practice)
                Self::in_memory().expect("Failed to create in-memory log reader")
            }
        }
    }

    async fn save_vote(&mut self, vote: &Vote<u64>) -> Result<(), StorageError<u64>> {
        let json = serde_json::to_string(vote).map_err(|e| StorageError::IO {
            source: StorageIOError::write(&e),
        })?;
        self.set_state_value("vote", &json)
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<u64>>, StorageError<u64>> {
        self.get_state_value("vote")?
            .map(|v| {
                serde_json::from_str(&v).map_err(|e| StorageError::IO {
                    source: StorageIOError::read(&e),
                })
            })
            .transpose()
    }

    async fn append<I>(
        &mut self,
        entries: I,
        callback: LogFlushed<TypeConfig>,
    ) -> Result<(), StorageError<u64>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + OptionalSend,
        I::IntoIter: OptionalSend,
    {
        let conn = self.conn.lock().unwrap();

        for entry in entries {
            let data = serde_json::to_vec(&entry).map_err(|e| StorageError::IO {
                source: StorageIOError::write(&e),
            })?;
            let term = entry.log_id.committed_leader_id().term;
            conn.execute(
                "INSERT OR REPLACE INTO raft_log (log_index, term, entry) VALUES (?1, ?2, ?3)",
                rusqlite::params![entry.log_id.index, term, data],
            )
            .map_err(|e| StorageError::IO {
                source: StorageIOError::write(&e),
            })?;
        }

        // Signal that the data is flushed (SQLite WAL guarantees durability)
        callback.log_io_completed(Ok(()));

        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<u64>) -> Result<(), StorageError<u64>> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM raft_log WHERE log_index >= ?1",
            rusqlite::params![log_id.index],
        )
        .map_err(|e| StorageError::IO {
            source: StorageIOError::write(&e),
        })?;
        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<u64>) -> Result<(), StorageError<u64>> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM raft_log WHERE log_index <= ?1",
            rusqlite::params![log_id.index],
        )
        .map_err(|e| StorageError::IO {
            source: StorageIOError::write(&e),
        })?;

        drop(conn);
        let json = serde_json::to_string(&log_id).map_err(|e| StorageError::IO {
            source: StorageIOError::write(&e),
        })?;
        self.set_state_value("last_purged", &json)?;

        Ok(())
    }
}

impl openraft::storage::RaftLogReader<TypeConfig> for SqliteLogStore {
    async fn try_get_log_entries<
        RB: std::ops::RangeBounds<u64> + Clone + std::fmt::Debug + OptionalSend,
    >(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<TypeConfig>>, StorageError<u64>> {
        let start = match range.start_bound() {
            std::ops::Bound::Included(&v) => v,
            std::ops::Bound::Excluded(&v) => v + 1,
            std::ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            std::ops::Bound::Included(&v) => v + 1,
            std::ops::Bound::Excluded(&v) => v,
            std::ops::Bound::Unbounded => u64::MAX,
        };

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT entry FROM raft_log WHERE log_index >= ?1 AND log_index < ?2 ORDER BY log_index",
            )
            .map_err(|e| StorageError::IO {
                source: StorageIOError::read(&e),
            })?;

        let entries: Vec<Entry<TypeConfig>> = stmt
            .query_map(rusqlite::params![start, end], |row| {
                let data: Vec<u8> = row.get(0)?;
                Ok(data)
            })
            .map_err(|e| StorageError::IO {
                source: StorageIOError::read(&e),
            })?
            .filter_map(|r| r.ok())
            .filter_map(|data| serde_json::from_slice(&data).ok())
            .collect();

        Ok(entries)
    }
}
