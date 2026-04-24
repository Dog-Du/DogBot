use agent_runner::history::store::HistoryStore;
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
