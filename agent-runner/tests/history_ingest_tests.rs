use agent_runner::history::store::history_schema_sql;

#[test]
fn history_schema_sql_creates_two_history_tables_and_agent_view() {
    let sql = history_schema_sql();

    assert!(sql.contains("CREATE TABLE IF NOT EXISTS history_messages"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS history_read_grants"));
    assert!(sql.contains("ALTER TABLE history_messages ENABLE ROW LEVEL SECURITY"));
    assert!(sql.contains("CREATE OR REPLACE VIEW agent_read.messages"));
    assert!(!sql.contains("message_part_store"));
    assert!(!sql.contains("asset_store"));
    assert!(!sql.contains("conversation_ingest_state"));
}
