use std::sync::{Arc, Mutex};

use agent_runner::models::{
    ErrorResponse, MessageRequest, MessageResponse, RunRequest, RunResponse, ValidatedRunRequest,
};
use agent_runner::server::{Messenger, Runner, build_test_app_with_message_support};
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[derive(Default)]
struct CapturingMessenger {
    sent: Mutex<Vec<MessageRequest>>,
}

impl CapturingMessenger {
    fn all_requests(&self) -> Vec<MessageRequest> {
        self.sent.lock().unwrap().clone()
    }

    async fn wait_for_len(&self, expected: usize) -> Vec<MessageRequest> {
        for _ in 0..50 {
            let requests = self.all_requests();
            if requests.len() >= expected {
                return requests;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        self.all_requests()
    }

    async fn wait_for_last(&self) -> Option<MessageRequest> {
        self.wait_for_len(1).await.last().cloned()
    }
}

#[derive(Default)]
struct CountingRunner {
    calls: Mutex<usize>,
}

impl CountingRunner {
    fn call_count(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

#[async_trait]
impl Messenger for CapturingMessenger {
    async fn send(
        &self,
        request: MessageRequest,
        _session: agent_runner::session_store::SessionRecord,
    ) -> Result<MessageResponse, ErrorResponse> {
        self.sent.lock().unwrap().push(request);
        Ok(MessageResponse {
            status: "ok".into(),
            message_id: Some("msg-out-1".into()),
        })
    }
}

struct SuccessRunner;

#[async_trait]
impl Runner for SuccessRunner {
    async fn run(
        &self,
        _request: RunRequest,
        _validated: ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse> {
        Ok(RunResponse {
            status: "ok".into(),
            stdout: "收到".into(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
            duration_ms: 1,
        })
    }
}

#[async_trait]
impl Runner for CountingRunner {
    async fn run(
        &self,
        _request: RunRequest,
        _validated: ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse> {
        *self.calls.lock().unwrap() += 1;
        Ok(RunResponse {
            status: "ok".into(),
            stdout: "不应该调用 runner".into(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
            duration_ms: 1,
        })
    }
}

fn test_settings() -> agent_runner::config::Settings {
    let root = std::env::temp_dir().join(format!(
        "agent-runner-platform-delivery-tests-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
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
        wechatpadpro_base_url: "http://127.0.0.1:38849".into(),
        wechatpadpro_account_key: None,
        platform_qq_account_id: Some("qq:bot_uin:123".into()),
        platform_qq_bot_id: Some("123".into()),
        platform_wechatpadpro_account_id: Some("wechatpadpro:account:bot".into()),
        platform_wechatpadpro_bot_mention_names: vec!["DogDu".into()],
        max_concurrent_runs: 1,
        max_queue_depth: 1,
        session_db_path: root.join("state/runner.db").display().to_string(),
        history_db_path: root.join("state/history.db").display().to_string(),
        database_url: "postgres://dogbot_admin:change-me@127.0.0.1:5432/dogbot".into(),
        postgres_agent_reader_user: "dogbot_agent_reader".into(),
        postgres_agent_reader_password: "change-me-reader".into(),
        postgres_agent_reader_database_url: None,
        history_run_token_ttl_secs: 1800,
        history_retention_days: 180,
        admin_actor_ids: Vec::new(),
        container_cpu_cores: 4,
        container_memory_mb: 4096,
        container_disk_gb: 50,
        container_pids_limit: 256,
    }
}

#[tokio::test]
async fn wechat_ingress_runs_and_delivers_reply_message() {
    let messenger = Arc::new(CapturingMessenger::default());
    let app = build_test_app_with_message_support(
        Arc::new(SuccessRunner),
        messenger.clone(),
        test_settings(),
    );
    let payload = serde_json::json!({
        "message": {
            "msgId": "wx-1",
            "senderWxid": "wxid_user_1",
            "content": "帮我总结一下",
            "msgType": 1
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/platforms/wechatpadpro/events")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        messenger.wait_for_last().await,
        Some(MessageRequest {
            session_id: "wechatpadpro:private:wxid_user_1".into(),
            text: "收到".into(),
            reply_to_message_id: None,
            mention_user_id: None,
        })
    );
}

#[tokio::test]
async fn qq_private_status_command_bypasses_runner_and_returns_health_reply() {
    let runner = Arc::new(CountingRunner::default());
    let messenger = Arc::new(CapturingMessenger::default());
    let app =
        build_test_app_with_message_support(runner.clone(), messenger.clone(), test_settings());
    let payload = serde_json::json!({
        "time": 1_710_000_000,
        "post_type": "message",
        "message_type": "private",
        "user_id": 42,
        "message_id": 99,
        "raw_message": "/agent-status",
        "message": [
            {"type":"text","data":{"text":"/agent-status"}}
        ]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/platforms/qq/napcat/events")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(runner.call_count(), 0);
    let request = messenger.wait_for_last().await.expect("status reply");
    assert_eq!(request.session_id, "qq:private:42");
    assert!(request.text.starts_with("agent-runner ok\nrunning: 0/1"));
    assert!(request.text.contains("\nqueued: 0/1"));
    assert_eq!(request.reply_to_message_id, None);
    assert_eq!(request.mention_user_id, None);
}
