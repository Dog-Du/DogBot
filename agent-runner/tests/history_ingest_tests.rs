use agent_runner::{
    config::Settings,
    history::store::{HistoryReadGrant, HistoryStore, history_schema_sql},
    protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart},
};
use serde_json::json;

#[test]
fn history_schema_sql_creates_two_history_tables_and_agent_view() {
    let sql = history_schema_sql();

    assert!(sql.contains("CREATE TABLE IF NOT EXISTS history_messages"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS history_read_grants"));
    assert!(sql.contains("ALTER TABLE history_messages ENABLE ROW LEVEL SECURITY"));
    assert!(sql.contains("CREATE OR REPLACE VIEW agent_read.messages"));
    assert!(sql.contains("security_invoker = true"));
    assert!(!sql.contains("message_part_store"));
    assert!(!sql.contains("asset_store"));
    assert!(!sql.contains("conversation_ingest_state"));
}

#[test]
fn postgres_history_store_handles_grants_messages_and_cleanup() {
    let Some(mut settings) = postgres_test_settings() else {
        eprintln!("DOGBOT_TEST_DATABASE_URL unset; skipping postgres integration test");
        return;
    };
    let suffix = uuid::Uuid::new_v4().to_string();
    settings.postgres_agent_reader_user = format!("dogbot_agent_reader_{suffix}");

    let store = HistoryStore::open(&settings).expect("history store");
    store
        .initialize_schema()
        .expect("initialize history schema");

    let platform_account = format!("qq:bot_uin:test:{suffix}");
    let conversation_id = format!("qq:private:test:{suffix}");
    let actor_id = format!("qq:user:test:{suffix}");

    store
        .create_read_grant(HistoryReadGrant {
            platform_account: platform_account.clone(),
            conversation_id: Some(conversation_id.clone()),
            actor_id: actor_id.clone(),
            is_admin: false,
            ttl_secs: 1800,
        })
        .expect("create read grant");

    store
        .insert_canonical_event(&CanonicalEvent {
            platform: "qq".into(),
            platform_account: platform_account.clone(),
            conversation: conversation_id.clone(),
            actor: actor_id.clone(),
            event_id: format!("qq:event:{suffix}"),
            timestamp_epoch_secs: 1_700_000_000,
            kind: EventKind::Message {
                message: CanonicalMessage {
                    message_id: format!("msg-{suffix}"),
                    reply_to: None,
                    parts: vec![MessagePart::Text {
                        text: "hello history".into(),
                    }],
                    mentions: Vec::new(),
                    native_metadata: json!({"sender_display": "Tester"}),
                },
            },
            raw_native_payload: json!({"message": {"id": suffix}}),
        })
        .expect("insert canonical event");

    assert_eq!(
        store
            .message_count(&platform_account, &conversation_id)
            .expect("message count"),
        1
    );

    store.purge_expired().expect("purge expired history");
}

fn postgres_test_settings() -> Option<Settings> {
    let database_url = std::env::var("DOGBOT_TEST_DATABASE_URL").ok()?;
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.keep();
    Some(Settings {
        bind_addr: "127.0.0.1:8787".into(),
        default_timeout_secs: 120,
        max_timeout_secs: 300,
        container_name: "claude-runner".into(),
        image_name: "dogbot/claude-runner:local".into(),
        workspace_dir: root.join("workdir").display().to_string(),
        state_dir: root.join("state").display().to_string(),
        claude_prompt_root: root.join("claude-prompt").display().to_string(),
        anthropic_base_url: "http://127.0.0.1:8080/anthropic".into(),
        anthropic_api_key: "dummy".into(),
        bifrost_port: 8080,
        bifrost_provider_name: "primary".into(),
        bifrost_model: "primary/model-id".into(),
        bifrost_upstream_base_url: "https://example.com".into(),
        bifrost_upstream_api_key: "replace-me".into(),
        bifrost_upstream_provider_type: "anthropic".into(),
        napcat_api_base_url: "http://127.0.0.1:3001".into(),
        napcat_access_token: None,
        wechatpadpro_base_url: "http://127.0.0.1:38849".into(),
        wechatpadpro_account_key: None,
        platform_qq_account_id: None,
        platform_qq_bot_id: None,
        platform_wechatpadpro_account_id: None,
        platform_wechatpadpro_bot_mention_names: Vec::new(),
        max_concurrent_runs: 1,
        max_queue_depth: 1,
        global_rate_limit_per_minute: 10,
        user_rate_limit_per_minute: 3,
        conversation_rate_limit_per_minute: 5,
        session_db_path: root.join("state/runner.db").display().to_string(),
        history_db_path: root.join("state/history.db").display().to_string(),
        database_url,
        postgres_agent_reader_user: "dogbot_agent_reader_test".into(),
        postgres_agent_reader_password: "change-me-reader".into(),
        postgres_agent_reader_database_url: None,
        history_run_token_ttl_secs: 1800,
        history_retention_days: 180,
        admin_actor_ids: Vec::new(),
        container_cpu_cores: 4,
        container_memory_mb: 4096,
        container_disk_gb: 50,
        container_pids_limit: 256,
    })
}
