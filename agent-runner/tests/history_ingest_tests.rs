use agent_runner::history::store::HistoryStore;

#[test]
fn history_store_creates_message_and_ingest_tables() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("history.db");
    let store = HistoryStore::open(&db_path).unwrap();
    let tables = store.table_names().unwrap();

    assert!(tables.contains(&"message_store".to_string()));
    assert!(tables.contains(&"message_attachment".to_string()));
    assert!(tables.contains(&"asset_store".to_string()));
    assert!(tables.contains(&"conversation_ingest_state".to_string()));
}
