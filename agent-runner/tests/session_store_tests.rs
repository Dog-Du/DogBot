use agent_runner::session_store::SessionStore;
use rusqlite::Connection;

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
fn legacy_session_api_uses_conversation_scoped_storage() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("runner.db");
    let store = SessionStore::open(&db_path).unwrap();

    let first = store
        .get_or_create_session("qq-user-1", "qq", "qq:group:5566", "qq:user:1")
        .unwrap();
    let second = store
        .get_or_create_session("qq-user-2", "qq", "qq:group:5566", "qq:user:2")
        .unwrap();
    let third = store
        .get_or_create_session("qq-user-3", "qq", "qq:group:7788", "qq:user:3")
        .unwrap();

    assert_eq!(first.claude_session_id, second.claude_session_id);
    assert_ne!(first.claude_session_id, third.claude_session_id);
}

#[test]
fn legacy_external_session_id_does_not_override_conversation_scoping() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("runner.db");
    let store = SessionStore::open(&db_path).unwrap();

    let first = store
        .get_or_create_session("qq-user-1", "qq", "qq:private:1", "qq:user:1")
        .unwrap();
    let second = store
        .get_or_create_session("qq-user-1", "qq", "qq:private:2", "qq:user:1")
        .unwrap();

    assert_ne!(first.claude_session_id, second.claude_session_id);
}

#[test]
fn session_store_reset_session_rotates_claude_session_id() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("runner.db");
    let store = SessionStore::open(&db_path).unwrap();

    let first = store
        .get_or_create_session("qq-user-1", "qq", "private:1", "1")
        .unwrap();
    let reset = store
        .reset_session("qq-user-1", "qq", "private:1", "1")
        .unwrap();
    let fetched = store.get_session("qq-user-1").unwrap().unwrap();

    assert_ne!(first.claude_session_id, reset.claude_session_id);
    assert_eq!(reset.claude_session_id, fetched.claude_session_id);
    assert!(reset.is_new);
}

#[test]
fn group_sessions_are_keyed_by_conversation_not_actor() {
    let temp = tempfile::tempdir().unwrap();
    let store = SessionStore::open(temp.path().join("runner.db")).unwrap();

    let first = store
        .get_or_create_conversation_session("qq", "qq:bot_uin:123", "qq:group:5566")
        .unwrap();

    let second = store
        .get_or_create_conversation_session("qq", "qq:bot_uin:123", "qq:group:5566")
        .unwrap();

    let third = store
        .get_or_create_conversation_session("qq", "qq:bot_uin:123", "qq:group:7788")
        .unwrap();

    assert_eq!(first.claude_session_id, second.claude_session_id);
    assert_ne!(first.claude_session_id, third.claude_session_id);
}

#[test]
fn session_store_recreates_incompatible_legacy_schema() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("runner.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (
            external_session_id TEXT PRIMARY KEY,
            claude_session_id TEXT NOT NULL,
            platform TEXT NOT NULL,
            conversation_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            created_at_epoch_secs INTEGER NOT NULL,
            last_used_at_epoch_secs INTEGER NOT NULL
        );",
    )
    .unwrap();
    drop(conn);

    let store = SessionStore::open(&db_path).unwrap();
    let record = store
        .get_or_create_conversation_session("qq", "qq:bot_uin:123", "qq:group:5566")
        .unwrap();

    let conn = Connection::open(&db_path).unwrap();
    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(sessions)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(record.conversation_id, "qq:group:5566");
    assert!(columns.contains(&"session_key".to_string()));
    assert!(columns.contains(&"platform_account".to_string()));
    assert!(!columns.contains(&"external_session_id".to_string()));
}
