use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};

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
            "CREATE TABLE IF NOT EXISTS event_store (
                event_id TEXT PRIMARY KEY,
                platform TEXT NOT NULL,
                platform_account TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                actor_id TEXT NOT NULL,
                event_kind TEXT NOT NULL,
                created_at_epoch_secs INTEGER NOT NULL,
                raw_native_payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS message_store (
                message_id TEXT PRIMARY KEY,
                event_id TEXT NOT NULL,
                reply_to_message_id TEXT,
                plain_text TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS message_part_store (
                message_id TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                part_kind TEXT NOT NULL,
                text_value TEXT,
                asset_id TEXT,
                target_actor_id TEXT,
                target_message_id TEXT,
                PRIMARY KEY (message_id, ordinal)
            );
            CREATE TABLE IF NOT EXISTS message_relation_store (
                relation_id TEXT PRIMARY KEY,
                source_message_id TEXT NOT NULL,
                relation_kind TEXT NOT NULL,
                target_message_id TEXT,
                target_actor_id TEXT,
                emoji TEXT
            );
            CREATE TABLE IF NOT EXISTS asset_store (
                asset_id TEXT PRIMARY KEY,
                asset_kind TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                source_kind TEXT NOT NULL,
                source_value TEXT NOT NULL,
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
            params![conversation_id, enabled as i64, retention_days],
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
        let platform = platform_from_conversation_id(conversation_id);
        let event_id = event_id_for_message(message_id);
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "INSERT OR IGNORE INTO event_store (
                event_id,
                platform,
                platform_account,
                conversation_id,
                actor_id,
                event_kind,
                created_at_epoch_secs,
                raw_native_payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, 'message', ?6, ?7)",
            params![
                event_id,
                platform,
                "compat:unknown",
                conversation_id,
                actor_id,
                created_at_epoch_secs,
                "{}"
            ],
        )?;

        tx.execute(
            "INSERT OR IGNORE INTO message_store (
                message_id,
                event_id,
                reply_to_message_id,
                plain_text
            ) VALUES (?1, ?2, NULL, ?3)",
            params![message_id, event_id, normalized_text],
        )?;

        tx.execute(
            "INSERT OR IGNORE INTO message_part_store (
                message_id,
                ordinal,
                part_kind,
                text_value,
                asset_id,
                target_actor_id,
                target_message_id
            ) VALUES (?1, 0, 'text', ?2, NULL, NULL, NULL)",
            params![message_id, normalized_text],
        )?;

        tx.commit()
    }

    pub fn message_count(&self, conversation_id: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*)
             FROM message_store m
             INNER JOIN event_store e ON e.event_id = m.event_id
             WHERE e.conversation_id = ?1",
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
                    m.plain_text,
                    EXISTS(
                        SELECT 1
                        FROM message_part_store p
                        WHERE p.message_id = m.message_id
                            AND p.asset_id IS NOT NULL
                    ) AS has_attachment
             FROM message_store m
             INNER JOIN event_store e ON e.event_id = m.event_id
             WHERE e.conversation_id = ?1
             ORDER BY e.created_at_epoch_secs DESC, m.message_id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![conversation_id, limit as i64], |row| {
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
        source_value: &str,
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
            "INSERT OR REPLACE INTO asset_store (
                asset_id,
                asset_kind,
                mime_type,
                size_bytes,
                source_kind,
                source_value,
                availability_status
            ) VALUES (?1, 'image', 'image/png', 0, 'path', ?2, 'available')",
            params![asset_id, source_value],
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO message_part_store (
                message_id,
                ordinal,
                part_kind,
                text_value,
                asset_id,
                target_actor_id,
                target_message_id
            ) VALUES (?1, 1, 'asset', NULL, ?2, NULL, NULL)",
            params![message_id, asset_id],
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
            "DELETE FROM message_relation_store
             WHERE source_message_id IN (
                SELECT m.message_id
                FROM message_store m
                INNER JOIN event_store e ON e.event_id = m.event_id
                INNER JOIN conversation_ingest_state s ON s.conversation_id = e.conversation_id
                WHERE s.enabled = 1
                    AND e.created_at_epoch_secs < (?1 - (s.retention_days * 86_400))
             )
             OR target_message_id IN (
                SELECT m.message_id
                FROM message_store m
                INNER JOIN event_store e ON e.event_id = m.event_id
                INNER JOIN conversation_ingest_state s ON s.conversation_id = e.conversation_id
                WHERE s.enabled = 1
                    AND e.created_at_epoch_secs < (?1 - (s.retention_days * 86_400))
             )",
            params![now],
        )?;

        tx.execute(
            "DELETE FROM message_part_store
             WHERE message_id IN (
                SELECT m.message_id
                FROM message_store m
                INNER JOIN event_store e ON e.event_id = m.event_id
                INNER JOIN conversation_ingest_state s ON s.conversation_id = e.conversation_id
                WHERE s.enabled = 1
                    AND e.created_at_epoch_secs < (?1 - (s.retention_days * 86_400))
             )",
            params![now],
        )?;

        tx.execute(
            "DELETE FROM message_store
             WHERE message_id IN (
                SELECT m.message_id
                FROM message_store m
                INNER JOIN event_store e ON e.event_id = m.event_id
                INNER JOIN conversation_ingest_state s ON s.conversation_id = e.conversation_id
                WHERE s.enabled = 1
                    AND e.created_at_epoch_secs < (?1 - (s.retention_days * 86_400))
             )",
            params![now],
        )?;

        tx.execute(
            "DELETE FROM event_store
             WHERE event_kind = 'message'
               AND EXISTS (
                    SELECT 1
                    FROM conversation_ingest_state s
                    WHERE s.conversation_id = event_store.conversation_id
                      AND s.enabled = 1
                      AND event_store.created_at_epoch_secs < (?1 - (s.retention_days * 86_400))
               )",
            params![now],
        )?;

        tx.commit()
    }

    pub fn delete_orphaned_assets(&self) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM asset_store
             WHERE asset_id NOT IN (
                SELECT DISTINCT asset_id
                FROM message_part_store
                WHERE asset_id IS NOT NULL
             )",
            [],
        )?;
        Ok(())
    }
}

fn event_id_for_message(message_id: &str) -> String {
    format!("event::{message_id}")
}

fn platform_from_conversation_id(conversation_id: &str) -> &str {
    conversation_id.split(':').next().unwrap_or("unknown")
}

fn now_secs() -> rusqlite::Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?
        .as_secs() as i64)
}
