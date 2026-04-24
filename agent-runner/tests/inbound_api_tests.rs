use std::sync::Arc;

use agent_runner::inbound_models::InboundMessage;
use agent_runner::models::{
    ErrorResponse, MessageRequest, MessageResponse, RunRequest, RunResponse, ValidatedRunRequest,
};
use axum::{
    body::Body,
    body::to_bytes,
    http::{Request, StatusCode},
};
use rusqlite::Connection;
use serde_json::json;
use tower::ServiceExt;

struct MockRunner;

#[derive(Default)]
struct NoopMessenger;

#[async_trait::async_trait]
impl agent_runner::server::Runner for MockRunner {
    async fn run(
        &self,
        _request: RunRequest,
        _validated: ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse> {
        Ok(RunResponse {
            status: "ok".into(),
            stdout: "".into(),
            stderr: "".into(),
            exit_code: 0,
            timed_out: false,
            duration_ms: 0,
        })
    }
}

#[async_trait::async_trait]
impl agent_runner::server::Messenger for NoopMessenger {
    async fn send(
        &self,
        _request: MessageRequest,
        _session: agent_runner::session_store::SessionRecord,
    ) -> Result<MessageResponse, ErrorResponse> {
        Ok(MessageResponse {
            status: "ok".into(),
            message_id: Some("noop".into()),
        })
    }
}

fn test_settings() -> agent_runner::config::Settings {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.keep();
    agent_runner::config::Settings {
        bind_addr: "127.0.0.1:8787".into(),
        default_timeout_secs: 120,
        max_timeout_secs: 300,
        container_name: "claude-runner".into(),
        image_name: "dogbot/claude-runner:local".into(),
        workspace_dir: root.join("workdir").display().to_string(),
        state_dir: root.join("state").display().to_string(),
        claude_prompt_root: "./claude-prompt".into(),
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
        max_concurrent_runs: 1,
        max_queue_depth: 1,
        global_rate_limit_per_minute: 10,
        user_rate_limit_per_minute: 3,
        conversation_rate_limit_per_minute: 5,
        session_db_path: root.join("state/runner.db").display().to_string(),
        history_db_path: root.join("state/history.db").display().to_string(),
        container_cpu_cores: 4,
        container_memory_mb: 4096,
        container_disk_gb: 50,
        container_pids_limit: 256,
    }
}

fn base_inbound_message() -> InboundMessage {
    InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:1".into(),
        conversation_id: "qq:private:1".into(),
        actor_id: "qq:user:2".into(),
        message_id: "msg-1".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "hi".into(),
        mentions: vec![],
        is_group: false,
        is_private: true,
        timestamp_epoch_secs: 1_700_000_000,
    }
}

#[tokio::test]
async fn inbound_messages_endpoint_accepts_private_plain_text_trigger() {
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner),
        Arc::new(NoopMessenger),
        test_settings(),
    );
    let payload = serde_json::to_vec(&base_inbound_message()).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/inbound-messages")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "accepted");
}

#[tokio::test]
async fn inbound_messages_endpoint_returns_ignored_for_group_message_without_bot_mention() {
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner),
        Arc::new(NoopMessenger),
        test_settings(),
    );
    let mut message = base_inbound_message();
    message.conversation_id = "qq:group:100".into();
    message.normalized_text = "只是普通聊天".into();
    message.is_group = true;
    message.is_private = false;
    let payload = serde_json::to_vec(&message).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/inbound-messages")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ignored");
}

#[tokio::test]
async fn inbound_messages_endpoint_rejects_invalid_json() {
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner),
        Arc::new(NoopMessenger),
        test_settings(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/inbound-messages")
                .header("content-type", "application/json")
                .body(Body::from("{not-json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error_code"], "invalid_json");
}

#[tokio::test]
async fn inbound_api_persists_enabled_conversation_messages() {
    let settings = test_settings();
    let history_store =
        agent_runner::history::store::HistoryStore::open(&settings.history_db_path).unwrap();
    history_store
        .upsert_ingest_state("qq:group:100", true, 180)
        .unwrap();

    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner),
        Arc::new(NoopMessenger),
        settings.clone(),
    );
    let payload = serde_json::to_vec(&InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:group:100".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m-enabled-1".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "summarize".into(),
        mentions: vec!["qq:bot_uin:123".into()],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    })
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/inbound-messages")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let verifier =
        agent_runner::history::store::HistoryStore::open(&settings.history_db_path).unwrap();
    assert_eq!(verifier.message_count("qq:group:100").unwrap(), 1);
}

#[tokio::test]
async fn inbound_api_enables_history_on_first_valid_trigger() {
    let settings = test_settings();
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner),
        Arc::new(NoopMessenger),
        settings.clone(),
    );
    let payload = serde_json::to_vec(&InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:group:200".into(),
        actor_id: "qq:user:9".into(),
        message_id: "m-enable-1".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "summarize".into(),
        mentions: vec!["qq:bot_uin:123".into()],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 2,
    })
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/inbound-messages")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let verifier =
        agent_runner::history::store::HistoryStore::open(&settings.history_db_path).unwrap();
    assert!(verifier.ingest_enabled("qq:group:200").unwrap());
    assert_eq!(verifier.message_count("qq:group:200").unwrap(), 1);
}

#[tokio::test]
async fn inbound_api_persists_canonical_history_for_mentions_and_reply() {
    let settings = test_settings();
    let history_store =
        agent_runner::history::store::HistoryStore::open(&settings.history_db_path).unwrap();
    history_store
        .upsert_ingest_state("qq:group:300", true, 180)
        .unwrap();

    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner),
        Arc::new(NoopMessenger),
        settings.clone(),
    );
    let payload = serde_json::to_vec(&InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:group:300".into(),
        actor_id: "qq:user:9".into(),
        message_id: "m-canonical-1".into(),
        reply_to_message_id: Some("parent-42".into()),
        raw_segments_json: json!([
            {"type": "reply", "data": {"id": "parent-42"}},
            {"type": "at", "data": {"qq": "123"}},
            {"type": "text", "data": {"text": " summarize this"}}
        ])
        .to_string(),
        normalized_text: "summarize this".into(),
        mentions: vec!["qq:bot_uin:123".into()],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1234,
    })
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/inbound-messages")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let conn = Connection::open(&settings.history_db_path).unwrap();
    let (platform_account, raw_payload_json): (String, String) = conn
        .query_row(
            "SELECT platform_account, raw_native_payload_json
             FROM event_store
             WHERE event_id = 'event::m-canonical-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let raw_payload: serde_json::Value = serde_json::from_str(&raw_payload_json).unwrap();
    assert_eq!(platform_account, "qq:bot_uin:123");
    assert_eq!(raw_payload["raw_segments"][1]["type"], "at");
    assert_eq!(raw_payload["platform_account"], "qq:bot_uin:123");

    let (reply_to_message_id, plain_text): (Option<String>, String) = conn
        .query_row(
            "SELECT reply_to_message_id, plain_text
             FROM message_store
             WHERE message_id = 'm-canonical-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(reply_to_message_id.as_deref(), Some("parent-42"));
    assert_eq!(plain_text, "summarize this");

    let parts: Vec<(String, Option<String>, Option<String>)> = conn
        .prepare(
            "SELECT part_kind, text_value, target_actor_id
             FROM message_part_store
             WHERE message_id = 'm-canonical-1'
             ORDER BY ordinal",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(parts.iter().any(|(kind, _, actor)| {
        kind == "mention" && actor.as_deref() == Some("qq:bot_uin:123")
    }));
    assert!(parts.iter().any(|(kind, text, _)| {
        kind == "text"
            && text
                .as_deref()
                .is_some_and(|value| value.contains("summarize"))
    }));

    let relations: Vec<(String, Option<String>, Option<String>)> = conn
        .prepare(
            "SELECT relation_kind, target_message_id, target_actor_id
             FROM message_relation_store
             WHERE source_message_id = 'm-canonical-1'
             ORDER BY relation_kind, COALESCE(target_message_id, target_actor_id)",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(relations.iter().any(|(kind, target_message, _)| {
        kind == "reply_to" && target_message.as_deref() == Some("parent-42")
    }));
    assert!(relations.iter().any(|(kind, _, target_actor)| {
        kind == "mention" && target_actor.as_deref() == Some("qq:bot_uin:123")
    }));
}
