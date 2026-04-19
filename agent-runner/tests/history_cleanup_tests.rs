use agent_runner::history::{cleanup::purge_expired_history, store::HistoryStore};

#[test]
fn cleanup_removes_expired_messages_but_keeps_live_assets() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("history.db");
    let store = HistoryStore::open(&db_path).unwrap();

    store
        .insert_expired_message_for_test("m1", "qq:group:100")
        .unwrap();
    store
        .insert_live_asset_for_test("asset-1", "/tmp/a.png")
        .unwrap();

    purge_expired_history(&store).unwrap();

    assert_eq!(store.message_count("qq:group:100").unwrap(), 0);
    assert_eq!(store.asset_count().unwrap(), 1);
}
