use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

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

    pub fn recent_rows(
        &self,
        conversation_id: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<(String, String, bool)>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.message_id,
                    m.normalized_text,
                    EXISTS(
                        SELECT 1
                        FROM message_attachment a
                        WHERE a.message_id = m.message_id
                    ) AS has_attachment
             FROM message_store m
             WHERE m.conversation_id = ?1
             ORDER BY m.created_at_epoch_secs DESC, m.message_id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![conversation_id, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
            ))
        })?;
        rows.collect()
    }

    pub fn insert_expired_message_for_test(
        &self,
        message_id: &str,
        conversation_id: &str,
    ) -> rusqlite::Result<()> {
        self.upsert_ingest_state(conversation_id, true, 1)?;
        let expired_timestamp = now_secs()? - 3 * 86_400;
        self.insert_message(
            message_id,
            conversation_id,
            "system",
            "expired test message",
            expired_timestamp,
        )
    }

    pub fn insert_live_asset_for_test(
        &self,
        asset_id: &str,
        storage_path: &str,
    ) -> rusqlite::Result<()> {
        let now = now_secs()?;
        let message_id = format!("asset-holder-{asset_id}");
        let conversation_id = "qq:private:test";

        self.upsert_ingest_state(conversation_id, true, 3650)?;
        self.insert_message(
            &message_id,
            conversation_id,
            "system",
            "live asset test",
            now,
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO asset_store (asset_id, storage_path, mime_type, availability_status)
             VALUES (?1, ?2, 'image/png', 'available')",
            rusqlite::params![asset_id, storage_path],
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO message_attachment (attachment_id, message_id, attachment_type, asset_id)
             VALUES (?1, ?2, 'image', ?3)",
            rusqlite::params![format!("attachment-{asset_id}"), message_id, asset_id],
        )?;
        Ok(())
    }

    pub fn asset_count(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM asset_store", [], |row| {
                row.get::<_, i64>(0)
            })
    }

    pub fn delete_expired_messages(&self) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let now = now_secs()?;

        tx.execute(
            "DELETE FROM message_attachment
             WHERE message_id IN (
                SELECT m.message_id
                FROM message_store m
                INNER JOIN conversation_ingest_state s ON s.conversation_id = m.conversation_id
                WHERE s.enabled = 1
                    AND m.created_at_epoch_secs < (?1 - (s.retention_days * 86_400))
             )",
            rusqlite::params![now],
        )?;

        tx.execute(
            "DELETE FROM message_store
             WHERE EXISTS (
                SELECT 1
                FROM conversation_ingest_state s
                WHERE s.conversation_id = message_store.conversation_id
                    AND s.enabled = 1
                    AND message_store.created_at_epoch_secs < (?1 - (s.retention_days * 86_400))
             )",
            rusqlite::params![now],
        )?;

        tx.commit()
    }

    pub fn delete_orphaned_assets(&self) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM asset_store
             WHERE asset_id NOT IN (
                SELECT DISTINCT asset_id
                FROM message_attachment
                WHERE asset_id IS NOT NULL
             )",
            [],
        )?;
        Ok(())
    }
}

fn now_secs() -> rusqlite::Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?
        .as_secs() as i64)
}
