use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SessionStore {
    db_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub external_session_id: String,
    pub claude_session_id: String,
    pub platform: String,
    pub conversation_id: String,
    pub user_id: String,
    pub created_at_epoch_secs: i64,
    pub last_used_at_epoch_secs: i64,
    pub is_new: bool,
}

#[derive(Debug, Error)]
pub enum SessionStoreError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(
        "external session_id {external_session_id} already belongs to platform={platform}, conversation_id={conversation_id}, user_id={user_id}"
    )]
    SessionConflict {
        external_session_id: String,
        platform: String,
        conversation_id: String,
        user_id: String,
    },
}

impl SessionStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionStoreError> {
        let db_path = path.as_ref().to_path_buf();
        let store = Self { db_path };
        store.initialize()?;
        Ok(store)
    }

    pub fn get_or_create_session(
        &self,
        external_session_id: &str,
        platform: &str,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let conn = self.open_connection()?;
        let now = epoch_now();
        let existing = self.fetch_session(&conn, external_session_id)?;

        if let Some(mut record) = existing {
            ensure_session_identity(&record, platform, conversation_id, user_id)?;
            conn.execute(
                "UPDATE sessions SET last_used_at_epoch_secs = ?1 WHERE external_session_id = ?2",
                params![now, external_session_id],
            )?;
            record.last_used_at_epoch_secs = now;
            return Ok(record);
        }

        let claude_session_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO sessions (
                external_session_id,
                claude_session_id,
                platform,
                conversation_id,
                user_id,
                created_at_epoch_secs,
                last_used_at_epoch_secs
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(external_session_id) DO NOTHING",
            params![
                external_session_id,
                claude_session_id,
                platform,
                conversation_id,
                user_id,
                now,
                now
            ],
        )?;

        let inserted = conn.changes() > 0;
        if !inserted {
            let mut record = self
                .fetch_session(&conn, external_session_id)?
                .expect("session should exist after insert conflict");
            ensure_session_identity(&record, platform, conversation_id, user_id)?;
            conn.execute(
                "UPDATE sessions SET last_used_at_epoch_secs = ?1 WHERE external_session_id = ?2",
                params![now, external_session_id],
            )?;
            record.last_used_at_epoch_secs = now;
            return Ok(record);
        }

        Ok(SessionRecord {
            external_session_id: external_session_id.to_string(),
            claude_session_id,
            platform: platform.to_string(),
            conversation_id: conversation_id.to_string(),
            user_id: user_id.to_string(),
            created_at_epoch_secs: now,
            last_used_at_epoch_secs: now,
            is_new: true,
        })
    }

    pub fn get_session(
        &self,
        external_session_id: &str,
    ) -> Result<Option<SessionRecord>, SessionStoreError> {
        let conn = self.open_connection()?;
        self.fetch_session(&conn, external_session_id)
    }

    pub fn reset_session(
        &self,
        external_session_id: &str,
        platform: &str,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let conn = self.open_connection()?;
        let now = epoch_now();
        let existing = self
            .fetch_session(&conn, external_session_id)?
            .ok_or_else(|| SessionStoreError::SessionConflict {
                external_session_id: external_session_id.to_string(),
                platform: platform.to_string(),
                conversation_id: conversation_id.to_string(),
                user_id: user_id.to_string(),
            })?;

        ensure_session_identity(&existing, platform, conversation_id, user_id)?;

        let claude_session_id = Uuid::new_v4().to_string();
        conn.execute(
            "UPDATE sessions
             SET claude_session_id = ?1,
                 created_at_epoch_secs = ?2,
                 last_used_at_epoch_secs = ?2
             WHERE external_session_id = ?3",
            params![claude_session_id, now, external_session_id],
        )?;

        Ok(SessionRecord {
            external_session_id: external_session_id.to_string(),
            claude_session_id,
            platform: platform.to_string(),
            conversation_id: conversation_id.to_string(),
            user_id: user_id.to_string(),
            created_at_epoch_secs: now,
            last_used_at_epoch_secs: now,
            is_new: true,
        })
    }

    fn initialize(&self) -> Result<(), SessionStoreError> {
        let conn = self.open_connection()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                external_session_id TEXT PRIMARY KEY,
                claude_session_id TEXT NOT NULL,
                platform TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                created_at_epoch_secs INTEGER NOT NULL,
                last_used_at_epoch_secs INTEGER NOT NULL
            );",
        )?;
        Ok(())
    }

    fn open_connection(&self) -> Result<Connection, SessionStoreError> {
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&self.db_path)?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(conn)
    }

    fn fetch_session(
        &self,
        conn: &Connection,
        external_session_id: &str,
    ) -> Result<Option<SessionRecord>, SessionStoreError> {
        Ok(conn
            .query_row(
                "SELECT claude_session_id, platform, conversation_id, user_id, created_at_epoch_secs, last_used_at_epoch_secs
                 FROM sessions WHERE external_session_id = ?1",
                params![external_session_id],
                |row| {
                    Ok(SessionRecord {
                        external_session_id: external_session_id.to_string(),
                        claude_session_id: row.get(0)?,
                        platform: row.get(1)?,
                        conversation_id: row.get(2)?,
                        user_id: row.get(3)?,
                        created_at_epoch_secs: row.get(4)?,
                        last_used_at_epoch_secs: row.get(5)?,
                        is_new: false,
                    })
                },
            )
            .optional()?)
    }
}

fn epoch_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_secs() as i64
}

fn ensure_session_identity(
    record: &SessionRecord,
    platform: &str,
    conversation_id: &str,
    user_id: &str,
) -> Result<(), SessionStoreError> {
    if record.platform == platform
        && record.conversation_id == conversation_id
        && record.user_id == user_id
    {
        return Ok(());
    }

    Err(SessionStoreError::SessionConflict {
        external_session_id: record.external_session_id.clone(),
        platform: record.platform.clone(),
        conversation_id: record.conversation_id.clone(),
        user_id: record.user_id.clone(),
    })
}
