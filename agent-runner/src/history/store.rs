use std::path::Path;

use rusqlite::Connection;

pub struct HistoryStore {
    conn: Connection,
}

impl HistoryStore {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS message_store (
                message_id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                actor_id TEXT NOT NULL,
                normalized_text TEXT NOT NULL,
                created_at_epoch_secs INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS message_attachment (
                attachment_id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                attachment_type TEXT NOT NULL,
                asset_id TEXT
            );
            CREATE TABLE IF NOT EXISTS asset_store (
                asset_id TEXT PRIMARY KEY,
                storage_path TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                availability_status TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS conversation_ingest_state (
                conversation_id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL,
                retention_days INTEGER NOT NULL
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn table_names(&self) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn upsert_ingest_state(
        &self,
        conversation_id: &str,
        enabled: bool,
        retention_days: i64,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO conversation_ingest_state (conversation_id, enabled, retention_days)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(conversation_id) DO UPDATE
             SET enabled = excluded.enabled, retention_days = excluded.retention_days",
            rusqlite::params![conversation_id, enabled as i64, retention_days],
        )?;
        Ok(())
    }

    pub fn ingest_enabled(&self, conversation_id: &str) -> rusqlite::Result<bool> {
        let mut stmt = self.conn.prepare(
            "SELECT enabled FROM conversation_ingest_state WHERE conversation_id = ?1 LIMIT 1",
        )?;
        let result = stmt.query_row([conversation_id], |row| row.get::<_, i64>(0));
        match result {
            Ok(value) => Ok(value != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub fn insert_message(
        &self,
        message_id: &str,
        conversation_id: &str,
        actor_id: &str,
        normalized_text: &str,
        created_at_epoch_secs: i64,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO message_store (
                message_id,
                conversation_id,
                actor_id,
                normalized_text,
                created_at_epoch_secs
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                message_id,
                conversation_id,
                actor_id,
                normalized_text,
                created_at_epoch_secs
            ],
        )?;
        Ok(())
    }

    pub fn message_count(&self, conversation_id: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM message_store WHERE conversation_id = ?1",
            [conversation_id],
            |row| row.get::<_, i64>(0),
        )
    }
}
