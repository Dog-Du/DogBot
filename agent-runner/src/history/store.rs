use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::{
    inbound_models::InboundMessage,
    protocol::{AssetRef, AssetSource, CanonicalEvent, CanonicalMessage, EventKind, MessagePart},
};

const EXPECTED_EVENT_STORE_COLUMNS: &[&str] = &[
    "event_id",
    "platform",
    "platform_account",
    "conversation_id",
    "actor_id",
    "event_kind",
    "created_at_epoch_secs",
    "raw_native_payload_json",
];
const EXPECTED_MESSAGE_STORE_COLUMNS: &[&str] = &[
    "message_id",
    "event_id",
    "reply_to_message_id",
    "plain_text",
];
const EXPECTED_MESSAGE_PART_STORE_COLUMNS: &[&str] = &[
    "message_id",
    "ordinal",
    "part_kind",
    "text_value",
    "asset_id",
    "target_actor_id",
    "target_message_id",
];
const EXPECTED_MESSAGE_RELATION_STORE_COLUMNS: &[&str] = &[
    "relation_id",
    "source_message_id",
    "relation_kind",
    "target_message_id",
    "target_actor_id",
    "emoji",
];
const EXPECTED_ASSET_STORE_COLUMNS: &[&str] = &[
    "asset_id",
    "asset_kind",
    "mime_type",
    "size_bytes",
    "source_kind",
    "source_value",
    "availability_status",
];
const EXPECTED_CONVERSATION_INGEST_STATE_COLUMNS: &[&str] = &[
    "platform_account",
    "conversation_id",
    "enabled",
    "retention_days",
];

pub struct HistoryStore {
    conn: Connection,
}

impl HistoryStore {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        initialize_history_schema(&conn)?;
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
        platform_account: &str,
        conversation_id: &str,
        enabled: bool,
        retention_days: i64,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO conversation_ingest_state (
                platform_account,
                conversation_id,
                enabled,
                retention_days
            ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(platform_account, conversation_id) DO UPDATE
             SET enabled = excluded.enabled, retention_days = excluded.retention_days",
            params![
                platform_account,
                conversation_id,
                enabled as i64,
                retention_days
            ],
        )?;
        Ok(())
    }

    pub fn ingest_enabled(
        &self,
        platform_account: &str,
        conversation_id: &str,
    ) -> rusqlite::Result<bool> {
        let mut stmt = self.conn.prepare(
            "SELECT enabled
             FROM conversation_ingest_state
             WHERE platform_account = ?1 AND conversation_id = ?2
             LIMIT 1",
        )?;
        let result = stmt.query_row(params![platform_account, conversation_id], |row| {
            row.get::<_, i64>(0)
        });
        match result {
            Ok(value) => Ok(value != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub fn insert_inbound_message(&self, inbound: &InboundMessage) -> rusqlite::Result<()> {
        let event = canonical_event_from_inbound_message(inbound);
        self.insert_canonical_event(&event)
    }

    pub fn insert_canonical_event(&self, event: &CanonicalEvent) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO event_store (
                event_id,
                platform,
                platform_account,
                conversation_id,
                actor_id,
                event_kind,
                created_at_epoch_secs,
                raw_native_payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.event_id,
                event.platform,
                event.platform_account,
                event.conversation,
                event.actor,
                event.kind_name(),
                event.timestamp_epoch_secs,
                event.raw_native_payload.to_string(),
            ],
        )?;

        if let EventKind::Message { message } = &event.kind {
            write_message_rows(&tx, &event.event_id, message)?;
        }

        tx.commit()
    }

    pub fn message_count(
        &self,
        platform_account: &str,
        conversation_id: &str,
    ) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*)
             FROM message_store m
             INNER JOIN event_store e ON e.event_id = m.event_id
             WHERE e.platform_account = ?1 AND e.conversation_id = ?2",
            params![platform_account, conversation_id],
            |row| row.get::<_, i64>(0),
        )
    }

    pub fn recent_rows(
        &self,
        platform_account: &str,
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
             WHERE e.platform_account = ?1 AND e.conversation_id = ?2
             ORDER BY e.created_at_epoch_secs DESC, m.message_id DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            params![platform_account, conversation_id, limit as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                ))
            },
        )?;
        rows.collect()
    }

    pub fn insert_expired_message_for_test(
        &self,
        message_id: &str,
        conversation_id: &str,
    ) -> rusqlite::Result<()> {
        self.upsert_ingest_state("test:history", conversation_id, true, 1)?;
        let expired_timestamp = now_secs()? - 3 * 86_400;
        self.insert_canonical_event(&build_text_event_for_test(
            message_id,
            conversation_id,
            "system",
            "test:history",
            "expired test message",
            expired_timestamp,
        ))
    }

    pub fn insert_live_asset_for_test(
        &self,
        asset_id: &str,
        source_value: &str,
    ) -> rusqlite::Result<()> {
        let now = now_secs()?;
        let message_id = format!("asset-holder-{asset_id}");
        let conversation_id = "qq:private:test";

        self.upsert_ingest_state("test:history", conversation_id, true, 3650)?;
        self.insert_canonical_event(&build_text_event_for_test(
            &message_id,
            conversation_id,
            "system",
            "test:history",
            "live asset test",
            now,
        ))?;
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
                INNER JOIN conversation_ingest_state s
                    ON s.platform_account = e.platform_account
                   AND s.conversation_id = e.conversation_id
                WHERE s.enabled = 1
                    AND e.created_at_epoch_secs < (?1 - (s.retention_days * 86_400))
             )
             OR target_message_id IN (
                SELECT m.message_id
                FROM message_store m
                INNER JOIN event_store e ON e.event_id = m.event_id
                INNER JOIN conversation_ingest_state s
                    ON s.platform_account = e.platform_account
                   AND s.conversation_id = e.conversation_id
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
                INNER JOIN conversation_ingest_state s
                    ON s.platform_account = e.platform_account
                   AND s.conversation_id = e.conversation_id
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
                INNER JOIN conversation_ingest_state s
                    ON s.platform_account = e.platform_account
                   AND s.conversation_id = e.conversation_id
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
                    WHERE s.platform_account = event_store.platform_account
                      AND s.conversation_id = event_store.conversation_id
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

fn write_message_rows(
    tx: &rusqlite::Transaction<'_>,
    event_id: &str,
    message: &CanonicalMessage,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT OR REPLACE INTO message_store (
            message_id,
            event_id,
            reply_to_message_id,
            plain_text
        ) VALUES (?1, ?2, ?3, ?4)",
        params![
            message.message_id,
            event_id,
            message.reply_to,
            storage_plain_text(message),
        ],
    )?;

    tx.execute(
        "DELETE FROM message_part_store WHERE message_id = ?1",
        params![message.message_id],
    )?;
    tx.execute(
        "DELETE FROM message_relation_store WHERE source_message_id = ?1",
        params![message.message_id],
    )?;

    let mut mention_targets = Vec::new();
    for (ordinal, part) in message.parts.iter().enumerate() {
        let (part_kind, text_value, asset, target_actor_id, target_message_id) =
            part_row_values(part);

        if let Some(asset) = asset.as_ref() {
            upsert_asset_row(tx, asset)?;
        }

        tx.execute(
            "INSERT INTO message_part_store (
                message_id,
                ordinal,
                part_kind,
                text_value,
                asset_id,
                target_actor_id,
                target_message_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                message.message_id,
                ordinal as i64,
                part_kind,
                text_value,
                asset.map(|asset| asset.asset_id.clone()),
                target_actor_id,
                target_message_id,
            ],
        )?;

        if let MessagePart::Mention { actor_id, .. } = part {
            mention_targets.push(actor_id.clone());
        }

        if let MessagePart::Quote {
            target_message_id, ..
        } = part
        {
            upsert_relation_row(
                tx,
                &format!("quote::{}::{ordinal}", message.message_id),
                &message.message_id,
                "quote",
                Some(target_message_id),
                None,
                None,
            )?;
        }
    }

    if let Some(reply_to) = message.reply_to.as_deref() {
        upsert_relation_row(
            tx,
            &format!("reply::{}", message.message_id),
            &message.message_id,
            "reply_to",
            Some(reply_to),
            None,
            None,
        )?;
    }

    for actor_id in &message.mentions {
        if mention_targets.iter().any(|existing| existing == actor_id) {
            continue;
        }
        upsert_relation_row(
            tx,
            &format!("mention::{}::{actor_id}", message.message_id),
            &message.message_id,
            "mention",
            None,
            Some(actor_id),
            None,
        )?;
    }

    for actor_id in mention_targets {
        upsert_relation_row(
            tx,
            &format!("mention::{}::{actor_id}", message.message_id),
            &message.message_id,
            "mention",
            None,
            Some(&actor_id),
            None,
        )?;
    }

    Ok(())
}

fn upsert_relation_row(
    tx: &rusqlite::Transaction<'_>,
    relation_id: &str,
    source_message_id: &str,
    relation_kind: &str,
    target_message_id: Option<&str>,
    target_actor_id: Option<&str>,
    emoji: Option<&str>,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT OR REPLACE INTO message_relation_store (
            relation_id,
            source_message_id,
            relation_kind,
            target_message_id,
            target_actor_id,
            emoji
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            relation_id,
            source_message_id,
            relation_kind,
            target_message_id,
            target_actor_id,
            emoji
        ],
    )?;
    Ok(())
}

fn upsert_asset_row(tx: &rusqlite::Transaction<'_>, asset: &AssetRef) -> rusqlite::Result<()> {
    let (source_kind, source_value) = asset_source_columns(&asset.source);
    tx.execute(
        "INSERT OR REPLACE INTO asset_store (
            asset_id,
            asset_kind,
            mime_type,
            size_bytes,
            source_kind,
            source_value,
            availability_status
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'available')",
        params![
            asset.asset_id,
            asset.kind,
            asset.mime,
            asset.size_bytes as i64,
            source_kind,
            source_value,
        ],
    )?;
    Ok(())
}

fn part_row_values(
    part: &MessagePart,
) -> (
    &'static str,
    Option<&str>,
    Option<&AssetRef>,
    Option<&str>,
    Option<&str>,
) {
    match part {
        MessagePart::Text { text } => ("text", Some(text), None, None, None),
        MessagePart::Mention { actor_id, display } => {
            ("mention", Some(display), None, Some(actor_id), None)
        }
        MessagePart::Image { asset } => ("image", None, Some(asset), None, None),
        MessagePart::File { asset } => ("file", None, Some(asset), None, None),
        MessagePart::Voice { asset } => ("voice", None, Some(asset), None, None),
        MessagePart::Video { asset } => ("video", None, Some(asset), None, None),
        MessagePart::Sticker { asset } => ("sticker", None, Some(asset), None, None),
        MessagePart::Quote {
            target_message_id,
            excerpt,
        } => ("quote", Some(excerpt), None, None, Some(target_message_id)),
    }
}

fn canonical_event_from_inbound_message(inbound: &InboundMessage) -> CanonicalEvent {
    let raw_segments = parse_raw_segments_json(&inbound.raw_segments_json);
    let parts = build_message_parts(inbound, &raw_segments);
    let message = CanonicalMessage {
        message_id: inbound.message_id.clone(),
        reply_to: inbound.reply_to_message_id.clone(),
        parts,
        mentions: inbound.mentions.clone(),
        native_metadata: json!({
            "is_group": inbound.is_group,
            "is_private": inbound.is_private,
        }),
    };

    CanonicalEvent {
        platform: inbound.platform.clone(),
        platform_account: inbound.platform_account.clone(),
        conversation: inbound.conversation_id.clone(),
        actor: inbound.actor_id.clone(),
        event_id: event_id_for_message(&inbound.message_id),
        timestamp_epoch_secs: inbound.timestamp_epoch_secs,
        kind: EventKind::Message { message },
        raw_native_payload: json!({
            "raw_segments": raw_segments,
            "platform_account": inbound.platform_account.clone(),
            "normalized_text": inbound.normalized_text.clone(),
            "mentions": inbound.mentions.clone(),
            "reply_to_message_id": inbound.reply_to_message_id.clone(),
            "conversation_id": inbound.conversation_id.clone(),
            "actor_id": inbound.actor_id.clone(),
        }),
    }
}

fn build_text_event_for_test(
    message_id: &str,
    conversation_id: &str,
    actor_id: &str,
    platform_account: &str,
    text: &str,
    created_at_epoch_secs: i64,
) -> CanonicalEvent {
    CanonicalEvent {
        platform: platform_from_conversation_id(conversation_id).to_string(),
        platform_account: platform_account.to_string(),
        conversation: conversation_id.to_string(),
        actor: actor_id.to_string(),
        event_id: event_id_for_message(message_id),
        timestamp_epoch_secs: created_at_epoch_secs,
        kind: EventKind::Message {
            message: CanonicalMessage {
                message_id: message_id.to_string(),
                reply_to: None,
                parts: vec![MessagePart::Text {
                    text: text.to_string(),
                }],
                mentions: Vec::new(),
                native_metadata: json!({}),
            },
        },
        raw_native_payload: json!({
            "source": "history-store-test-helper",
        }),
    }
}

fn storage_plain_text(message: &CanonicalMessage) -> String {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            MessagePart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn build_message_parts(inbound: &InboundMessage, raw_segments: &Value) -> Vec<MessagePart> {
    let mut parts = Vec::new();
    let mut mention_index = 0usize;

    if let Some(segments) = raw_segments.as_array() {
        for (ordinal, segment) in segments.iter().enumerate() {
            if let Some(part) = message_part_from_segment(
                &inbound.platform,
                inbound,
                segment,
                ordinal,
                &mut mention_index,
            ) {
                parts.push(part);
            }
        }
    }

    while let Some(actor_id) = inbound.mentions.get(mention_index) {
        parts.push(MessagePart::Mention {
            display: infer_mention_display_from_actor(actor_id),
            actor_id: actor_id.clone(),
        });
        mention_index += 1;
    }

    if parts.is_empty() && !inbound.normalized_text.is_empty() {
        parts.push(MessagePart::Text {
            text: inbound.normalized_text.clone(),
        });
    }

    parts
}

fn message_part_from_segment(
    platform: &str,
    inbound: &InboundMessage,
    segment: &Value,
    ordinal: usize,
    mention_index: &mut usize,
) -> Option<MessagePart> {
    let segment_type = segment_type(segment)?;
    match segment_type {
        "text" => segment_text(segment).map(|text| MessagePart::Text {
            text: text.to_string(),
        }),
        "at" | "mention" => {
            let actor_id = inbound
                .mentions
                .get(*mention_index)
                .cloned()
                .or_else(|| infer_mention_actor_id(platform, segment))?;
            *mention_index += 1;
            Some(MessagePart::Mention {
                display: infer_mention_display(segment, &actor_id),
                actor_id,
            })
        }
        "image" | "file" | "record" | "voice" | "video" | "sticker" | "face" => {
            asset_from_segment(segment, &inbound.message_id, ordinal).map(
                |asset| match segment_type {
                    "image" => MessagePart::Image { asset },
                    "file" => MessagePart::File { asset },
                    "record" | "voice" => MessagePart::Voice { asset },
                    "video" => MessagePart::Video { asset },
                    _ => MessagePart::Sticker { asset },
                },
            )
        }
        "reply" => inbound
            .reply_to_message_id
            .as_deref()
            .map(|target_message_id| MessagePart::Quote {
                target_message_id: target_message_id.to_string(),
                excerpt: String::new(),
            }),
        _ => None,
    }
}

fn asset_from_segment(segment: &Value, message_id: &str, ordinal: usize) -> Option<AssetRef> {
    let (source, source_value) =
        if let Some(path) = segment_string(segment, &["path", "local_path"]) {
            (
                AssetSource::WorkspacePath(path.to_string()),
                path.to_string(),
            )
        } else if let Some(url) = segment_string(segment, &["url"]) {
            (AssetSource::ExternalUrl(url.to_string()), url.to_string())
        } else if let Some(handle) = segment_string(segment, &["file", "file_id", "id"]) {
            (
                AssetSource::PlatformNativeHandle(handle.to_string()),
                handle.to_string(),
            )
        } else {
            return None;
        };

    let kind = segment_type(segment)?.to_string();
    Some(AssetRef {
        asset_id: format!("asset::{message_id}::{ordinal}"),
        kind: normalize_asset_kind(&kind).to_string(),
        mime: infer_mime_type(&kind, &source_value),
        size_bytes: 0,
        source,
    })
}

fn normalize_asset_kind(kind: &str) -> &str {
    match kind {
        "record" => "voice",
        "face" => "sticker",
        other => other,
    }
}

fn infer_mime_type(kind: &str, source_value: &str) -> String {
    if let Some(ext) = source_value.rsplit('.').next() {
        let lower = ext.to_ascii_lowercase();
        let mapped = match lower.as_str() {
            "png" => Some("image/png"),
            "jpg" | "jpeg" => Some("image/jpeg"),
            "gif" => Some("image/gif"),
            "webp" => Some("image/webp"),
            "mp4" => Some("video/mp4"),
            "mp3" => Some("audio/mpeg"),
            "wav" => Some("audio/wav"),
            "ogg" => Some("audio/ogg"),
            "pdf" => Some("application/pdf"),
            _ => None,
        };
        if let Some(mime) = mapped {
            return mime.to_string();
        }
    }

    match normalize_asset_kind(kind) {
        "image" => "image/*".to_string(),
        "voice" => "audio/*".to_string(),
        "video" => "video/*".to_string(),
        "sticker" => "application/x-sticker".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

fn asset_source_columns(source: &AssetSource) -> (&'static str, &str) {
    match source {
        AssetSource::WorkspacePath(value) => ("workspace_path", value),
        AssetSource::ManagedStore(value) => ("managed_store", value),
        AssetSource::ExternalUrl(value) => ("external_url", value),
        AssetSource::PlatformNativeHandle(value) => ("platform_native_handle", value),
        AssetSource::BridgeHandle(value) => ("bridge_handle", value),
    }
}

fn parse_raw_segments_json(raw_segments_json: &str) -> Value {
    serde_json::from_str(raw_segments_json)
        .unwrap_or_else(|_| Value::String(raw_segments_json.to_string()))
}

fn segment_type(segment: &Value) -> Option<&str> {
    segment
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn segment_text(segment: &Value) -> Option<&str> {
    segment_string(segment, &["text"])
}

fn segment_string<'a>(segment: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(value) = segment
            .get("data")
            .and_then(|data| data.get(*key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(value);
        }

        if let Some(value) = segment
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(value);
        }
    }

    None
}

fn infer_mention_actor_id(platform: &str, segment: &Value) -> Option<String> {
    let raw_id = segment_string(segment, &["qq", "wxid", "user_id", "actor_id"])?;
    if raw_id.contains(':') {
        return Some(raw_id.to_string());
    }

    match platform {
        "qq" => Some(format!("qq:bot_uin:{raw_id}")),
        "wechatpadpro" => Some(format!("wechatpadpro:user:{raw_id}")),
        _ => Some(raw_id.to_string()),
    }
}

fn infer_mention_display(segment: &Value, actor_id: &str) -> String {
    if let Some(display) = segment_string(segment, &["display", "name"]) {
        return display.to_string();
    }

    let suffix = actor_id.rsplit(':').next().unwrap_or(actor_id);
    format!("@{suffix}")
}

fn infer_mention_display_from_actor(actor_id: &str) -> String {
    let suffix = actor_id.rsplit(':').next().unwrap_or(actor_id);
    format!("@{suffix}")
}

fn initialize_history_schema(conn: &Connection) -> rusqlite::Result<()> {
    if history_schema_requires_reset(conn)? {
        drop_history_schema(conn)?;
    }
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
            platform_account TEXT NOT NULL,
            conversation_id TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            retention_days INTEGER NOT NULL,
            PRIMARY KEY (platform_account, conversation_id)
        );",
    )?;
    Ok(())
}

fn history_schema_requires_reset(conn: &Connection) -> rusqlite::Result<bool> {
    if table_columns(conn, "message_attachment")?.is_some() {
        return Ok(true);
    }

    for (table_name, expected_columns) in [
        ("event_store", EXPECTED_EVENT_STORE_COLUMNS),
        ("message_store", EXPECTED_MESSAGE_STORE_COLUMNS),
        ("message_part_store", EXPECTED_MESSAGE_PART_STORE_COLUMNS),
        (
            "message_relation_store",
            EXPECTED_MESSAGE_RELATION_STORE_COLUMNS,
        ),
        ("asset_store", EXPECTED_ASSET_STORE_COLUMNS),
        (
            "conversation_ingest_state",
            EXPECTED_CONVERSATION_INGEST_STATE_COLUMNS,
        ),
    ] {
        if table_columns(conn, table_name)?.is_some_and(|columns| columns != expected_columns) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn drop_history_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS message_attachment;
         DROP TABLE IF EXISTS message_relation_store;
         DROP TABLE IF EXISTS message_part_store;
         DROP TABLE IF EXISTS message_store;
         DROP TABLE IF EXISTS event_store;
         DROP TABLE IF EXISTS asset_store;
         DROP TABLE IF EXISTS conversation_ingest_state;",
    )?;
    Ok(())
}

fn table_columns(conn: &Connection, table_name: &str) -> rusqlite::Result<Option<Vec<String>>> {
    let pragma = format!("PRAGMA table_info({table_name})");
    let mut stmt = conn.prepare(&pragma)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let columns: Vec<String> = rows.collect::<Result<_, _>>()?;
    if columns.is_empty() {
        return Ok(None);
    }
    Ok(Some(columns))
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
