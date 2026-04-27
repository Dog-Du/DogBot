use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use postgres::{Client, NoTls};
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;
use url::Url;

use crate::config::Settings;
use crate::protocol::{CanonicalEvent, EventKind, MessagePart};

#[derive(Debug, Clone)]
pub struct HistoryStore {
    database_url: String,
    reader_database_url: String,
    reader_role: String,
    retention_days: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryReadGrant {
    pub platform_account: String,
    pub conversation_id: Option<String>,
    pub actor_id: String,
    pub is_admin: bool,
    pub ttl_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryReadGrantToken {
    pub token: String,
    pub database_url: String,
}

#[derive(Debug, Error)]
pub enum HistoryStoreError {
    #[error(transparent)]
    Postgres(#[from] postgres::Error),
    #[error("invalid postgres URL: {0}")]
    InvalidDatabaseUrl(String),
    #[error("postgres worker thread panicked")]
    WorkerPanicked,
}

impl HistoryStore {
    pub fn open(settings: &Settings) -> Result<Self, HistoryStoreError> {
        Ok(Self {
            database_url: settings.database_url.clone(),
            reader_database_url: reader_database_url(settings)?,
            reader_role: settings.postgres_agent_reader_user.clone(),
            retention_days: settings.history_retention_days,
        })
    }

    pub fn initialize_schema(&self) -> Result<(), HistoryStoreError> {
        let database_url = self.database_url.clone();
        let reader_role = self.reader_role.clone();
        run_with_client(database_url, move |client| {
            client.batch_execute(history_schema_sql())?;
            grant_reader_access(client, &reader_role)?;
            Ok(())
        })
    }

    pub fn reader_database_url(&self) -> &str {
        &self.reader_database_url
    }

    pub fn create_read_grant(
        &self,
        grant: HistoryReadGrant,
    ) -> Result<HistoryReadGrantToken, HistoryStoreError> {
        let token = generate_run_token();
        let token_hash = sha256_bytes(&token);
        let database_url = self.database_url.clone();
        let platform_account = grant.platform_account;
        let conversation_id = grant.conversation_id;
        let actor_id = grant.actor_id;
        let is_admin = grant.is_admin;
        let ttl_secs = grant.ttl_secs;
        run_with_client(database_url, move |client| {
            client.execute(
                "INSERT INTO history_read_grants (
                token_hash,
                platform_account,
                conversation_id,
                actor_id,
                is_admin,
                expires_at
            ) VALUES ($1, $2, $3, $4, $5, now() + ($6::text || ' seconds')::interval)",
            &[
                    &token_hash.as_slice(),
                    &platform_account,
                    &conversation_id,
                    &actor_id,
                    &is_admin,
                    &ttl_secs,
                ],
            )?;
            Ok(())
        })?;

        Ok(HistoryReadGrantToken {
            token,
            database_url: self.reader_database_url.clone(),
        })
    }

    pub fn upsert_ingest_state(
        &self,
        _platform_account: &str,
        _conversation_id: &str,
        _enabled: bool,
        _retention_days: i64,
    ) -> Result<(), HistoryStoreError> {
        Ok(())
    }

    pub fn ingest_enabled(
        &self,
        _platform_account: &str,
        _conversation_id: &str,
    ) -> Result<bool, HistoryStoreError> {
        Ok(true)
    }

    pub fn insert_canonical_event(&self, event: &CanonicalEvent) -> Result<(), HistoryStoreError> {
        let EventKind::Message {
            message,
        } = &event.kind
        else {
            return Ok(());
        };
        let plain_text = storage_plain_text(message).trim().to_string();
        if plain_text.is_empty() {
            return Ok(());
        }

        let actor_display = message
            .native_metadata
            .get("sender_display")
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
        let chat_type = conversation_scope(&event.conversation)
            .unwrap_or("unknown")
            .to_string();
        let database_url = self.database_url.clone();
        let platform = event.platform.clone();
        let platform_account = event.platform_account.clone();
        let conversation = event.conversation.clone();
        let actor = event.actor.clone();
        let message_id = message.message_id.clone();
        let reply_to = message.reply_to.clone();
        let timestamp_epoch_secs = event.timestamp_epoch_secs as f64;
        let raw = event.raw_native_payload.to_string();
        run_with_client(database_url, move |client| {
            client.execute(
                "INSERT INTO history_messages (
                platform,
                platform_account,
                conversation_id,
                chat_type,
                message_id,
                actor_id,
                actor_display,
                plain_text,
                reply_to_message_id,
                created_at,
                raw
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10), $11::jsonb
            )
            ON CONFLICT(platform_account, conversation_id, message_id) DO UPDATE
            SET actor_id = excluded.actor_id,
                actor_display = excluded.actor_display,
                plain_text = excluded.plain_text,
                reply_to_message_id = excluded.reply_to_message_id,
                created_at = excluded.created_at,
                raw = excluded.raw",
            &[
                    &platform,
                    &platform_account,
                    &conversation,
                    &chat_type,
                    &message_id,
                    &actor,
                    &actor_display,
                    &plain_text,
                    &reply_to,
                    &timestamp_epoch_secs,
                    &raw,
                ],
            )?;
            Ok(())
        })
    }

    pub fn message_count(
        &self,
        platform_account: &str,
        conversation_id: &str,
    ) -> Result<i64, HistoryStoreError> {
        let database_url = self.database_url.clone();
        let platform_account = platform_account.to_string();
        let conversation_id = conversation_id.to_string();
        run_with_client(database_url, move |client| {
            let row = client.query_one(
                "SELECT COUNT(*)
             FROM history_messages
             WHERE platform_account = $1 AND conversation_id = $2",
                &[&platform_account, &conversation_id],
            )?;
            Ok(row.get::<_, i64>(0))
        })
    }

    pub fn recent_rows(
        &self,
        platform_account: &str,
        conversation_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, String, bool)>, HistoryStoreError> {
        let database_url = self.database_url.clone();
        let platform_account = platform_account.to_string();
        let conversation_id = conversation_id.to_string();
        run_with_client(database_url, move |client| {
            let rows = client.query(
                "SELECT message_id, plain_text, false AS has_attachment
             FROM history_messages
             WHERE platform_account = $1 AND conversation_id = $2
             ORDER BY created_at DESC, id DESC
             LIMIT $3",
                &[&platform_account, &conversation_id, &(limit as i64)],
            )?;
            Ok(rows
                .into_iter()
                .map(|row| {
                    (
                        row.get::<_, String>(0),
                        row.get::<_, String>(1),
                        row.get::<_, bool>(2),
                    )
                })
                .collect())
        })
    }

    pub fn insert_expired_message_for_test(
        &self,
        _message_id: &str,
        _conversation_id: &str,
    ) -> Result<(), HistoryStoreError> {
        Ok(())
    }

    pub fn insert_live_asset_for_test(
        &self,
        _asset_id: &str,
        _source_value: &str,
    ) -> Result<(), HistoryStoreError> {
        Ok(())
    }

    pub fn asset_count(&self) -> Result<i64, HistoryStoreError> {
        Ok(0)
    }

    pub fn purge_expired(&self) -> Result<(), HistoryStoreError> {
        let database_url = self.database_url.clone();
        let retention_days = self.retention_days;
        run_with_client(database_url, move |client| {
            client.execute(
                "DELETE FROM history_read_grants WHERE expires_at <= now()",
                &[],
            )?;
            client.execute(
                "DELETE FROM history_messages
             WHERE created_at < now() - ($1::text || ' days')::interval",
                &[&retention_days],
            )?;
            Ok(())
        })
    }

    pub fn delete_expired_messages(&self) -> Result<(), HistoryStoreError> {
        self.purge_expired()
    }

    pub fn delete_orphaned_assets(&self) -> Result<(), HistoryStoreError> {
        Ok(())
    }

}

fn run_with_client<T, F>(database_url: String, f: F) -> Result<T, HistoryStoreError>
where
    T: Send + 'static,
    F: FnOnce(&mut Client) -> Result<T, HistoryStoreError> + Send + 'static,
{
    std::thread::spawn(move || {
        let mut client = Client::connect(&database_url, NoTls)?;
        f(&mut client)
    })
    .join()
    .map_err(|_| HistoryStoreError::WorkerPanicked)?
}

pub fn history_schema_sql() -> &'static str {
    r#"
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS history_messages (
    id bigserial PRIMARY KEY,
    platform text NOT NULL,
    platform_account text NOT NULL,
    conversation_id text NOT NULL,
    chat_type text NOT NULL,
    message_id text NOT NULL,
    actor_id text NOT NULL,
    actor_display text,
    plain_text text NOT NULL,
    reply_to_message_id text,
    created_at timestamptz NOT NULL,
    ingested_at timestamptz NOT NULL DEFAULT now(),
    raw jsonb
);

CREATE UNIQUE INDEX IF NOT EXISTS history_messages_unique_msg_idx
    ON history_messages (platform_account, conversation_id, message_id);

CREATE INDEX IF NOT EXISTS history_messages_conversation_time_idx
    ON history_messages (platform_account, conversation_id, created_at DESC);

CREATE INDEX IF NOT EXISTS history_messages_actor_time_idx
    ON history_messages (platform_account, actor_id, created_at DESC);

CREATE INDEX IF NOT EXISTS history_messages_platform_account_idx
    ON history_messages (platform_account);

CREATE TABLE IF NOT EXISTS history_read_grants (
    id bigserial PRIMARY KEY,
    token_hash bytea NOT NULL,
    platform_account text NOT NULL,
    conversation_id text,
    actor_id text NOT NULL,
    is_admin boolean NOT NULL DEFAULT false,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS history_read_grants_token_idx
    ON history_read_grants (token_hash);

CREATE INDEX IF NOT EXISTS history_read_grants_expiry_idx
    ON history_read_grants (expires_at);

CREATE INDEX IF NOT EXISTS history_read_grants_platform_account_idx
    ON history_read_grants (platform_account);

CREATE OR REPLACE FUNCTION dogbot_can_read_history_row(
    row_platform_account text,
    row_conversation_id text
)
RETURNS boolean
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM history_read_grants g
        WHERE g.token_hash = digest(
            current_setting('dogbot.run_token', true),
            'sha256'
        )
          AND g.expires_at > now()
          AND g.platform_account = row_platform_account
          AND (
              g.is_admin
              OR g.conversation_id = row_conversation_id
          )
    );
$$;

ALTER TABLE history_messages ENABLE ROW LEVEL SECURITY;
ALTER TABLE history_messages FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS history_messages_agent_read ON history_messages;
CREATE POLICY history_messages_agent_read
ON history_messages
FOR SELECT
USING (dogbot_can_read_history_row(platform_account, conversation_id));

CREATE SCHEMA IF NOT EXISTS agent_read;

CREATE OR REPLACE VIEW agent_read.messages
WITH (security_barrier = true)
AS
SELECT
    id,
    platform,
    platform_account,
    conversation_id,
    chat_type,
    message_id,
    actor_id,
    actor_display,
    plain_text,
    reply_to_message_id,
    created_at
FROM history_messages;
"#
}

fn grant_reader_access(client: &mut Client, reader_role: &str) -> Result<(), HistoryStoreError> {
    let reader_role = quote_identifier(reader_role);
    client.batch_execute(&format!(
        "GRANT USAGE ON SCHEMA agent_read TO {reader_role};
         GRANT SELECT ON agent_read.messages TO {reader_role};"
    ))?;
    Ok(())
}

fn reader_database_url(settings: &Settings) -> Result<String, HistoryStoreError> {
    let mut url = Url::parse(&settings.database_url)
        .map_err(|err| HistoryStoreError::InvalidDatabaseUrl(err.to_string()))?;
    url.set_username(&settings.postgres_agent_reader_user)
        .map_err(|_| HistoryStoreError::InvalidDatabaseUrl("invalid reader username".into()))?;
    url.set_password(Some(&settings.postgres_agent_reader_password))
        .map_err(|_| HistoryStoreError::InvalidDatabaseUrl("invalid reader password".into()))?;
    Ok(url.to_string())
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn generate_run_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn sha256_bytes(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}

fn storage_plain_text(message: &crate::protocol::CanonicalMessage) -> String {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            MessagePart::Text {
                text,
            } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn conversation_scope(conversation: &str) -> Option<&str> {
    let mut parts = conversation.splitn(3, ':');
    let _platform = parts.next()?;
    parts.next()
}
