use std::time::{SystemTime, UNIX_EPOCH};

use postgres::{Client, NoTls};
use thiserror::Error;
use uuid::Uuid;

use crate::config::Settings;

#[derive(Debug, Clone)]
pub struct SessionStore {
    database_url: String,
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
    Postgres(#[from] postgres::Error),
    #[error("postgres worker thread panicked")]
    WorkerPanicked,
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
    pub fn open(settings: &Settings) -> Result<Self, SessionStoreError> {
        Self::open_database_url(settings.database_url.clone())
    }

    pub fn open_database_url(database_url: impl Into<String>) -> Result<Self, SessionStoreError> {
        Ok(Self {
            database_url: database_url.into(),
        })
    }

    pub fn initialize_schema(&self) -> Result<(), SessionStoreError> {
        let database_url = self.database_url.clone();
        run_with_client(database_url, |client| {
            client.batch_execute(session_schema_sql())?;
            Ok(())
        })
    }

    pub fn get_or_create_conversation_session(
        &self,
        platform: &str,
        platform_account: &str,
        conversation_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let database_url = self.database_url.clone();
        let platform = platform.to_string();
        let platform_account = platform_account.to_string();
        let conversation_id = conversation_id.to_string();
        run_with_client(database_url, move |client| {
            let session_key =
                conversation_session_key(&platform, &platform_account, &conversation_id);
            get_or_create_by_key(
                client,
                &session_key,
                &platform,
                &platform_account,
                &conversation_id,
                &session_key,
            )
        })
    }

    pub fn reset_conversation_session(
        &self,
        platform: &str,
        platform_account: &str,
        conversation_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let database_url = self.database_url.clone();
        let platform = platform.to_string();
        let platform_account = platform_account.to_string();
        let conversation_id = conversation_id.to_string();
        run_with_client(database_url, move |client| {
            let session_key =
                conversation_session_key(&platform, &platform_account, &conversation_id);
            reset_by_key(
                client,
                &session_key,
                &platform,
                &platform_account,
                &conversation_id,
                &session_key,
            )
        })
    }

    pub fn get_or_create_bound_session(
        &self,
        external_session_id: &str,
        platform: &str,
        platform_account: &str,
        conversation_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let database_url = self.database_url.clone();
        let external_session_id = external_session_id.to_string();
        let platform = platform.to_string();
        let platform_account = platform_account.to_string();
        let conversation_id = conversation_id.to_string();
        run_with_client(database_url, move |client| {
            let session_key =
                conversation_session_key(&platform, &platform_account, &conversation_id);
            validate_external_session_binding_with_client(
                client,
                &external_session_id,
                &platform,
                &platform_account,
                &conversation_id,
            )?;
            let record = get_or_create_by_key(
                client,
                &session_key,
                &platform,
                &platform_account,
                &conversation_id,
                &external_session_id,
            )?;
            upsert_session_alias(client, &external_session_id, &session_key)?;
            Ok(record)
        })
    }

    pub fn bind_external_session_id(
        &self,
        external_session_id: &str,
        platform: &str,
        platform_account: &str,
        conversation_id: &str,
    ) -> Result<(), SessionStoreError> {
        let database_url = self.database_url.clone();
        let external_session_id = external_session_id.to_string();
        let platform = platform.to_string();
        let platform_account = platform_account.to_string();
        let conversation_id = conversation_id.to_string();
        run_with_client(database_url, move |client| {
            let session_key =
                conversation_session_key(&platform, &platform_account, &conversation_id);
            validate_external_session_binding_with_client(
                client,
                &external_session_id,
                &platform,
                &platform_account,
                &conversation_id,
            )?;
            let record =
                fetch_session(client, &session_key, &external_session_id)?.ok_or_else(|| {
                    SessionStoreError::SessionConflict {
                        external_session_id: external_session_id.clone(),
                        platform: platform.clone(),
                        conversation_id: conversation_id.clone(),
                        user_id: String::new(),
                    }
                })?;
            ensure_session_identity(&record, &platform, &platform_account, &conversation_id)?;
            upsert_session_alias(client, &external_session_id, &session_key)
        })
    }

    pub fn validate_external_session_binding(
        &self,
        external_session_id: &str,
        platform: &str,
        platform_account: &str,
        conversation_id: &str,
    ) -> Result<(), SessionStoreError> {
        let database_url = self.database_url.clone();
        let external_session_id = external_session_id.to_string();
        let platform = platform.to_string();
        let platform_account = platform_account.to_string();
        let conversation_id = conversation_id.to_string();
        run_with_client(database_url, move |client| {
            validate_external_session_binding_with_client(
                client,
                &external_session_id,
                &platform,
                &platform_account,
                &conversation_id,
            )
        })
    }

    pub fn get_session(
        &self,
        external_session_id: &str,
    ) -> Result<Option<SessionRecord>, SessionStoreError> {
        let database_url = self.database_url.clone();
        let external_session_id = external_session_id.to_string();
        run_with_client(database_url, move |client| {
            if let Some(session_key) = lookup_session_alias(client, &external_session_id)? {
                return fetch_session(client, &session_key, &external_session_id);
            }

            fetch_session(client, &external_session_id, &external_session_id)
        })
    }

    pub fn reset_bound_session(
        &self,
        external_session_id: &str,
        platform: &str,
        platform_account: &str,
        conversation_id: &str,
    ) -> Result<SessionRecord, SessionStoreError> {
        let database_url = self.database_url.clone();
        let external_session_id = external_session_id.to_string();
        let platform = platform.to_string();
        let platform_account = platform_account.to_string();
        let conversation_id = conversation_id.to_string();
        run_with_client(database_url, move |client| {
            let session_key =
                conversation_session_key(&platform, &platform_account, &conversation_id);
            let record = reset_by_key(
                client,
                &session_key,
                &platform,
                &platform_account,
                &conversation_id,
                &external_session_id,
            )?;
            upsert_session_alias(client, &external_session_id, &session_key)?;
            Ok(record)
        })
    }
}

fn get_or_create_by_key(
    client: &mut Client,
    session_key: &str,
    platform: &str,
    platform_account: &str,
    conversation_id: &str,
    external_session_id: &str,
) -> Result<SessionRecord, SessionStoreError> {
    let now = epoch_now();
    if let Some(mut record) = fetch_session(client, session_key, external_session_id)? {
        ensure_session_identity(&record, platform, platform_account, conversation_id)?;
        client.execute(
            "UPDATE runner_sessions
             SET last_used_at_epoch_secs = $1
             WHERE session_key = $2",
            &[&now, &session_key],
        )?;
        record.last_used_at_epoch_secs = now;
        return Ok(record);
    }

    let claude_session_id = Uuid::new_v4().to_string();
    let inserted = client.execute(
        "INSERT INTO runner_sessions (
            session_key,
            claude_session_id,
            platform,
            platform_account,
            conversation_id,
            created_at_epoch_secs,
            last_used_at_epoch_secs
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT(session_key) DO NOTHING",
        &[
            &session_key,
            &claude_session_id,
            &platform,
            &platform_account,
            &conversation_id,
            &now,
            &now,
        ],
    )? > 0;

    if !inserted {
        let mut record = fetch_session(client, session_key, external_session_id)?
            .expect("session should exist after insert conflict");
        ensure_session_identity(&record, platform, platform_account, conversation_id)?;
        client.execute(
            "UPDATE runner_sessions
             SET last_used_at_epoch_secs = $1
             WHERE session_key = $2",
            &[&now, &session_key],
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
    client: &mut Client,
    session_key: &str,
    platform: &str,
    platform_account: &str,
    conversation_id: &str,
    external_session_id: &str,
) -> Result<SessionRecord, SessionStoreError> {
    let now = epoch_now();
    let existing = fetch_session(client, session_key, external_session_id)?.ok_or_else(|| {
        SessionStoreError::SessionConflict {
            external_session_id: external_session_id.to_string(),
            platform: platform.to_string(),
            conversation_id: conversation_id.to_string(),
            user_id: String::new(),
        }
    })?;

    ensure_session_identity(&existing, platform, platform_account, conversation_id)?;

    let claude_session_id = Uuid::new_v4().to_string();
    client.execute(
        "UPDATE runner_sessions
         SET claude_session_id = $1,
             created_at_epoch_secs = $2,
             last_used_at_epoch_secs = $2
         WHERE session_key = $3",
        &[&claude_session_id, &now, &session_key],
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

fn fetch_session(
    client: &mut Client,
    session_key: &str,
    external_session_id: &str,
) -> Result<Option<SessionRecord>, SessionStoreError> {
    let row = client.query_opt(
        "SELECT
            claude_session_id,
            platform,
            platform_account,
            conversation_id,
            created_at_epoch_secs,
            last_used_at_epoch_secs
         FROM runner_sessions
         WHERE session_key = $1",
        &[&session_key],
    )?;

    Ok(row.map(|row| {
        build_session_record(
            session_key,
            external_session_id,
            row.get(0),
            row.get(1),
            row.get(2),
            row.get(3),
            row.get(4),
            row.get(5),
            false,
        )
    }))
}

fn validate_external_session_binding_with_client(
    client: &mut Client,
    external_session_id: &str,
    platform: &str,
    platform_account: &str,
    conversation_id: &str,
) -> Result<(), SessionStoreError> {
    let desired_session_key = conversation_session_key(platform, platform_account, conversation_id);
    let Some(existing_session_key) = lookup_session_alias(client, external_session_id)? else {
        return Ok(());
    };

    if existing_session_key == desired_session_key {
        return Ok(());
    }

    let existing =
        fetch_session(client, &existing_session_key, external_session_id)?.ok_or_else(|| {
            SessionStoreError::SessionConflict {
                external_session_id: external_session_id.to_string(),
                platform: platform.to_string(),
                conversation_id: conversation_id.to_string(),
                user_id: String::new(),
            }
        })?;

    Err(SessionStoreError::SessionConflict {
        external_session_id: external_session_id.to_string(),
        platform: existing.platform,
        conversation_id: existing.conversation_id,
        user_id: existing.user_id,
    })
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
        platform_account,
        conversation_id,
        user_id: String::new(),
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
    client: &mut Client,
    external_session_id: &str,
    session_key: &str,
) -> Result<(), SessionStoreError> {
    client.execute(
        "INSERT INTO runner_session_aliases (external_session_id, session_key)
         VALUES ($1, $2)
         ON CONFLICT(external_session_id) DO UPDATE
         SET session_key = excluded.session_key",
        &[&external_session_id, &session_key],
    )?;
    Ok(())
}

fn lookup_session_alias(
    client: &mut Client,
    external_session_id: &str,
) -> Result<Option<String>, SessionStoreError> {
    Ok(client
        .query_opt(
            "SELECT session_key
             FROM runner_session_aliases
             WHERE external_session_id = $1",
            &[&external_session_id],
        )?
        .map(|row| row.get(0)))
}

fn run_with_client<T, F>(database_url: String, f: F) -> Result<T, SessionStoreError>
where
    T: Send + 'static,
    F: FnOnce(&mut Client) -> Result<T, SessionStoreError> + Send + 'static,
{
    std::thread::spawn(move || {
        let mut client = Client::connect(&database_url, NoTls)?;
        f(&mut client)
    })
    .join()
    .map_err(|_| SessionStoreError::WorkerPanicked)?
}

pub fn session_schema_sql() -> &'static str {
    r#"
CREATE TABLE IF NOT EXISTS runner_sessions (
    session_key text PRIMARY KEY,
    claude_session_id text NOT NULL,
    platform text NOT NULL,
    platform_account text NOT NULL,
    conversation_id text NOT NULL,
    created_at_epoch_secs bigint NOT NULL,
    last_used_at_epoch_secs bigint NOT NULL
);

CREATE INDEX IF NOT EXISTS runner_sessions_platform_account_conversation_idx
    ON runner_sessions (platform, platform_account, conversation_id);

CREATE TABLE IF NOT EXISTS runner_session_aliases (
    external_session_id text PRIMARY KEY,
    session_key text NOT NULL REFERENCES runner_sessions(session_key) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS runner_session_aliases_session_key_idx
    ON runner_session_aliases (session_key);
"#
}
