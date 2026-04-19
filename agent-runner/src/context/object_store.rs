use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ContextObjectStore {
    db_path: PathBuf,
}

#[derive(Debug, Error)]
pub enum ContextObjectStoreError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

impl ContextObjectStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ContextObjectStoreError> {
        let db_path = path.as_ref().to_path_buf();
        let store = Self { db_path };
        store.initialize()?;
        Ok(store)
    }

    pub fn table_names(&self) -> Result<Vec<String>, ContextObjectStoreError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
        )?;

        let mut names = Vec::new();
        let mut rows = stmt.query(params![])?;
        while let Some(row) = rows.next()? {
            names.push(row.get::<_, String>(0)?);
        }
        Ok(names)
    }

    pub fn insert_memory_candidate(
        &self,
        actor_id: &str,
        conversation_id: &str,
        candidate_json: &str,
    ) -> Result<String, ContextObjectStoreError> {
        let conn = self.open_connection()?;

        let candidate_id = Uuid::new_v4().to_string();
        let created_at_epoch_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        conn.execute(
            "INSERT INTO memory_candidates (
                candidate_id,
                actor_id,
                conversation_id,
                candidate_json,
                created_at_epoch_secs
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                candidate_id,
                actor_id,
                conversation_id,
                candidate_json,
                created_at_epoch_secs
            ],
        )?;

        Ok(candidate_id)
    }

    fn initialize(&self) -> Result<(), ContextObjectStoreError> {
        let conn = self.open_connection()?;

        // Avoid mutating DB-level PRAGMAs on every connection open (can contend on locks).
        // Configure journal mode once during store initialization.
        let journal_mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        if journal_mode.to_ascii_uppercase() != "WAL" {
            conn.pragma_update(None, "journal_mode", "WAL")?;
        }

        // Minimal schema for Phase A scaffolding.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS context_objects (
                object_id TEXT PRIMARY KEY,
                object_type TEXT NOT NULL,
                object_json TEXT NOT NULL,
                created_at_epoch_secs INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS memory_candidates (
                candidate_id TEXT PRIMARY KEY,
                actor_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                candidate_json TEXT NOT NULL,
                created_at_epoch_secs INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS conversation_authorizations (
                conversation_id TEXT NOT NULL,
                actor_id TEXT NOT NULL,
                scope TEXT NOT NULL,
                created_at_epoch_secs INTEGER NOT NULL,
                PRIMARY KEY (conversation_id, actor_id, scope)
            );",
        )?;

        self.migrate_legacy_memory_candidates_schema(&conn)?;

        Ok(())
    }

    fn migrate_legacy_memory_candidates_schema(
        &self,
        conn: &Connection,
    ) -> Result<(), ContextObjectStoreError> {
        let mut stmt = conn.prepare("PRAGMA table_info('memory_candidates')")?;
        let mut rows = stmt.query([])?;
        let mut has_content = false;
        let mut has_candidate_json = false;

        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            match name.as_str() {
                "content" => has_content = true,
                "candidate_json" => has_candidate_json = true,
                _ => {}
            }
        }

        if has_content && !has_candidate_json {
            conn.execute_batch(
                "ALTER TABLE memory_candidates RENAME TO memory_candidates_legacy;

                CREATE TABLE memory_candidates (
                    candidate_id TEXT PRIMARY KEY,
                    actor_id TEXT NOT NULL,
                    conversation_id TEXT NOT NULL,
                    candidate_json TEXT NOT NULL,
                    created_at_epoch_secs INTEGER NOT NULL
                );

                INSERT INTO memory_candidates (
                    candidate_id,
                    actor_id,
                    conversation_id,
                    candidate_json,
                    created_at_epoch_secs
                )
                SELECT
                    candidate_id,
                    actor_id,
                    conversation_id,
                    content,
                    created_at_epoch_secs
                FROM memory_candidates_legacy;

                DROP TABLE memory_candidates_legacy;",
            )?;
        }

        Ok(())
    }

    fn open_connection(&self) -> Result<Connection, ContextObjectStoreError> {
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&self.db_path)?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        Ok(conn)
    }
}
