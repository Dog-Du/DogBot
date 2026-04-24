use agent_runner::{
    history::store::HistoryStore,
    protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart},
};
use rusqlite::Connection;

#[test]
fn history_store_creates_message_and_ingest_tables() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("history.db");
    let store = HistoryStore::open(&db_path).unwrap();
    let tables = store.table_names().unwrap();

    assert!(tables.contains(&"event_store".to_string()));
    assert!(tables.contains(&"message_store".to_string()));
    assert!(tables.contains(&"message_part_store".to_string()));
    assert!(tables.contains(&"message_relation_store".to_string()));
    assert!(tables.contains(&"asset_store".to_string()));
    assert!(tables.contains(&"conversation_ingest_state".to_string()));
}

#[test]
fn history_store_creates_canonical_event_tables() {
    let temp = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(temp.path().join("history.db")).unwrap();
    let tables = store.table_names().unwrap();

    assert!(tables.contains(&"event_store".to_string()));
    assert!(tables.contains(&"message_store".to_string()));
    assert!(tables.contains(&"message_part_store".to_string()));
    assert!(tables.contains(&"message_relation_store".to_string()));
    assert!(tables.contains(&"asset_store".to_string()));
}

#[test]
fn history_store_recreates_incompatible_legacy_schema() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("history.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE message_store (
            message_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            actor_id TEXT NOT NULL,
            normalized_text TEXT NOT NULL,
            created_at_epoch_secs INTEGER NOT NULL
        );
        CREATE TABLE message_attachment (
            attachment_id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            attachment_type TEXT NOT NULL,
            asset_id TEXT
        );
        CREATE TABLE asset_store (
            asset_id TEXT PRIMARY KEY,
            storage_path TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            availability_status TEXT NOT NULL
        );
        CREATE TABLE conversation_ingest_state (
            conversation_id TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL,
            retention_days INTEGER NOT NULL
        );",
    )
    .unwrap();
    drop(conn);

    let store = HistoryStore::open(&db_path).unwrap();
    let tables = store.table_names().unwrap();

    assert!(tables.contains(&"event_store".to_string()));
    assert!(tables.contains(&"message_store".to_string()));
    assert!(tables.contains(&"message_part_store".to_string()));
    assert!(tables.contains(&"message_relation_store".to_string()));
    assert!(tables.contains(&"asset_store".to_string()));
    assert!(!tables.contains(&"message_attachment".to_string()));
}

#[test]
fn history_store_scopes_ingest_state_and_reads_by_platform_account() {
    let temp = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(temp.path().join("history.db")).unwrap();

    store
        .upsert_ingest_state("qq:bot_uin:123", "qq:group:100", true, 30)
        .unwrap();

    assert!(
        store
            .ingest_enabled("qq:bot_uin:123", "qq:group:100")
            .unwrap()
    );
    assert!(
        !store
            .ingest_enabled("qq:bot_uin:999", "qq:group:100")
            .unwrap()
    );

    for (event_id, message_id, platform_account, text) in [
        ("evt-1", "msg-1", "qq:bot_uin:123", "from bot 123"),
        ("evt-2", "msg-2", "qq:bot_uin:999", "from bot 999"),
    ] {
        store
            .insert_canonical_event(&CanonicalEvent {
                platform: "qq".into(),
                platform_account: platform_account.into(),
                conversation: "qq:group:100".into(),
                actor: "qq:user:1".into(),
                event_id: event_id.into(),
                timestamp_epoch_secs: 1,
                kind: EventKind::Message {
                    message: CanonicalMessage {
                        message_id: message_id.into(),
                        reply_to: None,
                        parts: vec![MessagePart::Text { text: text.into() }],
                        mentions: vec![],
                        native_metadata: serde_json::json!({}),
                    },
                },
                raw_native_payload: serde_json::json!({ "source": "history-ingest-test" }),
            })
            .unwrap();
    }

    assert_eq!(
        store
            .message_count("qq:bot_uin:123", "qq:group:100")
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .message_count("qq:bot_uin:999", "qq:group:100")
            .unwrap(),
        1
    );
}
