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
        content_root: "./content".into(),
        anthropic_base_url: "http://host.docker.internal:9000".into(),
        api_proxy_auth_token: "local-proxy-token".into(),
        napcat_api_base_url: "http://127.0.0.1:3001".into(),
        napcat_access_token: None,
        max_concurrent_runs: 1,
        max_queue_depth: 1,
        global_rate_limit_per_minute: 10,
        user_rate_limit_per_minute: 3,
        conversation_rate_limit_per_minute: 5,
        control_plane_db_path: root.join("state/control.db").display().to_string(),
        admin_actor_ids: vec!["qq:user:1".into()],
        session_db_path: root.join("state/runner.db").display().to_string(),
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
        normalized_text: "/agent hi".into(),
        mentions: vec![],
        is_group: false,
        is_private: true,
        timestamp_epoch_secs: 1_700_000_000,
    }
}

#[tokio::test]
async fn inbound_messages_endpoint_accepts_agent_trigger() {
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
async fn inbound_messages_endpoint_returns_ignored_for_non_trigger() {
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner),
        Arc::new(NoopMessenger),
        test_settings(),
    );
    let mut message = base_inbound_message();
    message.normalized_text = "只是普通聊天".into();
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
