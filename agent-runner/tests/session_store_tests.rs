use agent_runner::session_store::SessionStore;

#[test]
fn session_store_persists_existing_mapping() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("runner.db");
    let store = SessionStore::open(&db_path).unwrap();

    let first = store
        .get_or_create_session("qq-user-1", "qq", "private:1", "1")
        .unwrap();
    let second = store
        .get_or_create_session("qq-user-1", "qq", "private:1", "1")
        .unwrap();

    assert_eq!(first.external_session_id, "qq-user-1");
    assert_eq!(first.claude_session_id, second.claude_session_id);
    assert!(first.is_new);
    assert!(!second.is_new);
}

#[test]
fn session_store_uses_distinct_claude_ids_for_distinct_external_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("runner.db");
    let store = SessionStore::open(&db_path).unwrap();

    let first = store
        .get_or_create_session("qq-user-1", "qq", "private:1", "1")
        .unwrap();
    let second = store
        .get_or_create_session("qq-user-2", "qq", "private:2", "2")
        .unwrap();

    assert_ne!(first.claude_session_id, second.claude_session_id);
}

#[test]
fn session_store_rejects_identity_mismatch_for_existing_external_session() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("runner.db");
    let store = SessionStore::open(&db_path).unwrap();

    store
        .get_or_create_session("qq-user-1", "qq", "private:1", "1")
        .unwrap();

    let err = store
        .get_or_create_session("qq-user-1", "qq", "private:2", "2")
        .unwrap_err();

    assert!(err.to_string().contains("already belongs"));
}
