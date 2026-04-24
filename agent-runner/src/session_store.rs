use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;
use uuid::Uuid;

const LEGACY_PLATFORM_ACCOUNT: &str = "compat:legacy-platform-account";
const EXPECTED_SESSION_COLUMNS: &[&str] = &[
    "session_key",
    "claude_session_id",
    "platform",
    "platform_account",
    "conversation_id",
    "created_at_epoch_secs",
    "last_used_at_epoch_secs",
];
const EXPECTED_SESSION_ALIAS_COLUMNS: &[&str] = &["external_session_id", "session_key"];

#[derive(Debug, Clone)]
pub struct SessionStore {
    db_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub session_key: String,
    pub external_session_id: String,
    pub claude_session_id: String,
    pub platform: String,
    pub platform_account: String,
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

    pub fn get_or_create_conversation_session(
        &self,
        platform: &str,
        platform_account: &str,
        conversation_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let conn = self.open_connection()?;
        let session_key = conversation_session_key(platform, platform_account, conversation_id);
        self.get_or_create_by_key(
            &conn,
            &session_key,
            platform,
            platform_account,
            conversation_id,
            &session_key,
        )
    }

    pub fn get_or_create_session(
        &self,
        external_session_id: &str,
        platform: &str,
        conversation_id: &str,
        _user_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let conn = self.open_connection()?;
        let session_key =
            conversation_session_key(platform, LEGACY_PLATFORM_ACCOUNT, conversation_id);
        let record = self.get_or_create_by_key(
            &conn,
            &session_key,
            platform,
            LEGACY_PLATFORM_ACCOUNT,
            conversation_id,
            external_session_id,
        )?;
        upsert_session_alias(&conn, external_session_id, &session_key)?;
        Ok(record)
    }

    pub fn get_session(
        &self,
        external_session_id: &str,
    ) -> Result<Option<SessionRecord>, SessionStoreError> {
        let conn = self.open_connection()?;
        if let Some(session_key) = lookup_session_alias(&conn, external_session_id)? {
            return self.fetch_session(&conn, &session_key, external_session_id);
        }

        self.fetch_session(&conn, external_session_id, external_session_id)
    }

    pub fn reset_session(
        &self,
        external_session_id: &str,
        platform: &str,
        conversation_id: &str,
        _user_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let conn = self.open_connection()?;
        let session_key =
            conversation_session_key(platform, LEGACY_PLATFORM_ACCOUNT, conversation_id);
        let record = self.reset_by_key(
            &conn,
            &session_key,
            platform,
            LEGACY_PLATFORM_ACCOUNT,
            conversation_id,
            external_session_id,
        )?;
        upsert_session_alias(&conn, external_session_id, &session_key)?;
        Ok(record)
    }

    fn get_or_create_by_key(
        &self,
        conn: &Connection,
        session_key: &str,
        platform: &str,
        platform_account: &str,
        conversation_id: &str,
        external_session_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let now = epoch_now();
        let existing = self.fetch_session(conn, session_key, external_session_id)?;

        if let Some(mut record) = existing {
            ensure_session_identity(&record, platform, platform_account, conversation_id)?;
            conn.execute(
                "UPDATE sessions SET last_used_at_epoch_secs = ?1 WHERE session_key = ?2",
                params![now, session_key],
            )?;
            record.last_used_at_epoch_secs = now;
            return Ok(record);
        }

        let claude_session_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO sessions (
                session_key,
                claude_session_id,
                platform,
                platform_account,
                conversation_id,
                created_at_epoch_secs,
                last_used_at_epoch_secs
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(session_key) DO NOTHING",
            params![
                session_key,
                claude_session_id,
                platform,
                platform_account,
                conversation_id,
                now,
                now
            ],
        )?;

        let inserted = conn.changes() > 0;
        if !inserted {
            let mut record = self
                .fetch_session(conn, session_key, external_session_id)?
                .expect("session should exist after insert conflict");
            ensure_session_identity(&record, platform, platform_account, conversation_id)?;
            conn.execute(
                "UPDATE sessions SET last_used_at_epoch_secs = ?1 WHERE session_key = ?2",
                params![now, session_key],
            )?;
            record.last_used_at_epoch_secs = now;
            return Ok(record);
        }

        Ok(build_session_record(
            session_key,
            external_session_id,
            claude_session_id,
            platform.to_string(),
            platform_account.to_string(),
            conversation_id.to_string(),
            now,
            now,
            true,
        ))
    }

    fn reset_by_key(
        &self,
        conn: &Connection,
        session_key: &str,
        platform: &str,
        platform_account: &str,
        conversation_id: &str,
        external_session_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let now = epoch_now();
        let existing = self
            .fetch_session(conn, session_key, external_session_id)?
            .ok_or_else(|| SessionStoreError::SessionConflict {
                external_session_id: external_session_id.to_string(),
                platform: platform.to_string(),
                conversation_id: conversation_id.to_string(),
                user_id: platform_account.to_string(),
            })?;

        ensure_session_identity(&existing, platform, platform_account, conversation_id)?;

        let claude_session_id = Uuid::new_v4().to_string();
        conn.execute(
            "UPDATE sessions
             SET claude_session_id = ?1,
                 created_at_epoch_secs = ?2,
                 last_used_at_epoch_secs = ?2
             WHERE session_key = ?3",
            params![claude_session_id, now, session_key],
        )?;

        Ok(build_session_record(
            session_key,
            external_session_id,
            claude_session_id,
            platform.to_string(),
            platform_account.to_string(),
            conversation_id.to_string(),
            now,
            now,
            true,
        ))
    }

    fn initialize(&self) -> Result<(), SessionStoreError> {
        let conn = self.open_connection()?;
        if session_schema_requires_reset(&conn)? {
            drop_session_schema(&conn)?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                session_key TEXT PRIMARY KEY,
                claude_session_id TEXT NOT NULL,
                platform TEXT NOT NULL,
                platform_account TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                created_at_epoch_secs INTEGER NOT NULL,
                last_used_at_epoch_secs INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS session_aliases (
                external_session_id TEXT PRIMARY KEY,
                session_key TEXT NOT NULL
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
        session_key: &str,
        external_session_id: &str,
    ) -> Result<Option<SessionRecord>, SessionStoreError> {
        Ok(conn
            .query_row(
                "SELECT claude_session_id, platform, platform_account, conversation_id, created_at_epoch_secs, last_used_at_epoch_secs
                 FROM sessions WHERE session_key = ?1",
                params![session_key],
                |row| {
                    Ok(build_session_record(
                        session_key,
                        external_session_id,
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        false,
                    ))
                },
            )
            .optional()?)
    }
}

fn build_session_record(
    session_key: &str,
    external_session_id: &str,
    claude_session_id: String,
    platform: String,
    platform_account: String,
    conversation_id: String,
    created_at_epoch_secs: i64,
    last_used_at_epoch_secs: i64,
    is_new: bool,
) -> SessionRecord {
    SessionRecord {
        session_key: session_key.to_string(),
        external_session_id: external_session_id.to_string(),
        claude_session_id,
        platform,
        platform_account: platform_account.clone(),
        conversation_id,
        user_id: platform_account,
        created_at_epoch_secs,
        last_used_at_epoch_secs,
        is_new,
    }
}

fn conversation_session_key(
    platform: &str,
    platform_account: &str,
    conversation_id: &str,
) -> String {
    format!("conversation::{platform}::{platform_account}::{conversation_id}")
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
    platform_account: &str,
    conversation_id: &str,
) -> Result<(), SessionStoreError> {
    if record.platform == platform
        && record.platform_account == platform_account
        && record.conversation_id == conversation_id
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

fn upsert_session_alias(
    conn: &Connection,
    external_session_id: &str,
    session_key: &str,
) -> Result<(), SessionStoreError> {
    conn.execute(
        "INSERT INTO session_aliases (external_session_id, session_key)
         VALUES (?1, ?2)
         ON CONFLICT(external_session_id) DO UPDATE
         SET session_key = excluded.session_key",
        params![external_session_id, session_key],
    )?;
    Ok(())
}

fn lookup_session_alias(
    conn: &Connection,
    external_session_id: &str,
) -> Result<Option<String>, SessionStoreError> {
    Ok(conn
        .query_row(
            "SELECT session_key FROM session_aliases WHERE external_session_id = ?1",
            params![external_session_id],
            |row| row.get(0),
        )
        .optional()?)
}

fn session_schema_requires_reset(conn: &Connection) -> Result<bool, SessionStoreError> {
    if table_columns(conn, "sessions")?
        .is_some_and(|columns| columns != EXPECTED_SESSION_COLUMNS)
    {
        return Ok(true);
    }

    if table_columns(conn, "session_aliases")?
        .is_some_and(|columns| columns != EXPECTED_SESSION_ALIAS_COLUMNS)
    {
        return Ok(true);
    }

    Ok(false)
}

fn drop_session_schema(conn: &Connection) -> Result<(), SessionStoreError> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS session_aliases;
         DROP TABLE IF EXISTS sessions;",
    )?;
    Ok(())
}

fn table_columns(conn: &Connection, table_name: &str) -> Result<Option<Vec<String>>, SessionStoreError> {
    let pragma = format!("PRAGMA table_info({table_name})");
    let mut stmt = conn.prepare(&pragma)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let columns: Vec<String> = rows.collect::<Result<_, _>>()?;
    if columns.is_empty() {
        return Ok(None);
    }
    Ok(Some(columns))
}
