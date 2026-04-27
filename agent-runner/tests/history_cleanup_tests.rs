use agent_runner::history::store::history_schema_sql;

#[test]
fn history_schema_supports_expired_grant_and_message_cleanup_targets() {
    let sql = history_schema_sql();

    assert!(sql.contains("history_read_grants_expiry_idx"));
    assert!(sql.contains("created_at timestamptz NOT NULL"));
}
