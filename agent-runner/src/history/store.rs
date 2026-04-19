use std::path::Path;

use rusqlite::Connection;

pub struct HistoryStore {
    conn: Connection,
}

impl HistoryStore {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
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
}
