use std::sync::{Arc, Mutex};

use agent_runner::config::Settings;
use agent_runner::history::store::HistoryStore;
use agent_runner::models::{ErrorResponse, RunRequest, RunResponse, ValidatedRunRequest};
use agent_runner::protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart};
use agent_runner::server::{
    Messenger, Runner, build_test_app, build_test_app_with_message_support,
    build_test_app_with_settings,
};
use async_trait::async_trait;
use axum::{
    body::Body,
    body::to_bytes,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

#[derive(Default)]
struct CapturingRunner {
    request: Arc<Mutex<Option<RunRequest>>>,
    validated: Arc<Mutex<Option<ValidatedRunRequest>>>,
}

#[derive(Default)]
struct NullMessenger;

impl CapturingRunner {
    fn captured_request(&self) -> Option<RunRequest> {
        self.request.lock().expect("lock request").clone()
    }

    fn captured_validated(&self) -> Option<ValidatedRunRequest> {
        self.validated.lock().expect("lock validated").clone()
    }
}

#[async_trait]
impl Runner for CapturingRunner {
    async fn run(
        &self,
        request: RunRequest,
        validated: ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse> {
        *self.request.lock().expect("lock request") = Some(request);
        *self.validated.lock().expect("lock validated") = Some(validated);
        Ok(RunResponse {
            status: "ok".into(),
            stdout: "ok".into(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
            duration_ms: 0,
        })
    }
}

#[async_trait]
impl Messenger for NullMessenger {
    async fn send(
        &self,
        _request: agent_runner::models::MessageRequest,
        _session: agent_runner::session_store::SessionRecord,
    ) -> Result<agent_runner::models::MessageResponse, ErrorResponse> {
        Ok(agent_runner::models::MessageResponse {
            status: "ok".into(),
            message_id: Some("msg-out-1".into()),
        })
    }
}

fn base_request() -> RunRequest {
    RunRequest {
        platform: "qq".into(),
        platform_account_id: "qq:bot_uin:123".into(),
        conversation_id: "qq:private:1".into(),
        session_id: "qq:private:1:qq:user:1".into(),
        user_id: "qq:user:1".into(),
        chat_type: "private".into(),
        cwd: "/workspace".into(),
        prompt: "hello".into(),
        trigger_summary: Some("hello".into()),
        trigger_message_id: None,
        trigger_reply_to_message_id: None,
        mention_refs: Vec::new(),
        reply_excerpt: None,
        timeout_secs: None,
    }
}

fn test_settings() -> Settings {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.keep();
    Settings {
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
        container_cpu_cores: 4,
        container_memory_mb: 4096,
        container_disk_gb: 50,
        container_pids_limit: 256,
    }
}

fn seed_history_message(store: &HistoryStore, message_id: &str, text: &str) {
    store
        .insert_canonical_event(&CanonicalEvent {
            platform: "qq".into(),
            platform_account: "qq:bot_uin:123".into(),
            conversation: "qq:private:1".into(),
            actor: "qq:user:1".into(),
            event_id: format!("event::{message_id}"),
            timestamp_epoch_secs: 1,
            kind: EventKind::Message {
                message: CanonicalMessage {
                    message_id: message_id.into(),
                    reply_to: None,
                    parts: vec![MessagePart::Text { text: text.into() }],
                    mentions: vec![],
                    native_metadata: serde_json::json!({}),
                },
            },
            raw_native_payload: serde_json::json!({
                "source": "context-run-test",
            }),
        })
        .expect("seed canonical history");
}

#[tokio::test]
async fn run_endpoint_builds_prompt_envelope_for_runner() {
    let runner = Arc::new(CapturingRunner::default());
    let app = build_test_app(runner.clone());
    let payload = serde_json::to_vec(&base_request()).expect("serialize request");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("build request"),
        )
        .await
        .expect("run request");

    assert_eq!(response.status(), StatusCode::OK);
    let captured_request = runner.captured_request().expect("captured request");
    let validated = runner
        .captured_validated()
        .expect("captured validated request");

    assert_eq!(captured_request.prompt, "hello");
    assert!(validated.system_prompt.contains("qq"));
    assert!(validated.system_prompt.contains("qq:bot_uin:123"));
    assert!(
        validated
            .system_prompt
            .contains("/state/claude-prompt/CLAUDE.md")
    );
    assert!(
        validated
            .system_prompt
            .contains("/state/claude-prompt/skills/reply-format/SKILL.md")
    );
    assert!(validated.system_prompt.contains("Do not use Markdown"));
    assert!(!validated.system_prompt.contains("qq:user:1"));
    assert!(validated.prompt.contains("qq:private:1"));
    assert!(validated.prompt.contains("qq:user:1"));
    assert!(validated.prompt.contains("hello"));
    assert!(validated.prompt.contains("\"trigger_message_id\":null"));
    assert!(validated.prompt.contains("\"mention_refs\":[]"));
}

#[tokio::test]
async fn run_endpoint_rejects_empty_user_id_for_context_pack() {
    let runner = Arc::new(CapturingRunner::default());
    let app = build_test_app(runner);
    let mut request = base_request();
    request.user_id = "   ".into();
    let payload = serde_json::to_vec(&request).expect("serialize request");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("build request"),
        )
        .await
        .expect("run request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let json: Value = serde_json::from_slice(&body).expect("decode response");
    assert_eq!(json["error_code"], "invalid_request");
    assert_eq!(json["message"], "user_id must be non-empty");
}

#[tokio::test]
async fn run_endpoint_normalizes_context_identifiers_before_dispatch() {
    let runner = Arc::new(CapturingRunner::default());
    let app = build_test_app(runner.clone());
    let mut request = base_request();
    request.user_id = "  qq:user:1  ".into();
    request.conversation_id = "  qq:private:1  ".into();
    request.platform_account_id = "  qq:bot_uin:123  ".into();
    let payload = serde_json::to_vec(&request).expect("serialize request");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("build request"),
        )
        .await
        .expect("run request");

    assert_eq!(response.status(), StatusCode::OK);
    let captured_request = runner.captured_request().expect("captured request");
    assert_eq!(captured_request.user_id, "qq:user:1");
    assert_eq!(captured_request.conversation_id, "qq:private:1");
    assert_eq!(captured_request.platform_account_id, "qq:bot_uin:123");
}

#[tokio::test]
async fn run_endpoint_does_not_inject_history_evidence_when_history_exists() {
    let settings = test_settings();
    let store = HistoryStore::open(&settings.history_db_path).expect("history store");
    seed_history_message(&store, "hist-1", "之前讨论过的上下文");

    let runner = Arc::new(CapturingRunner::default());
    let app = build_test_app_with_settings(runner.clone(), settings);
    let payload = serde_json::to_vec(&base_request()).expect("serialize request");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("build request"),
        )
        .await
        .expect("run request");

    assert_eq!(response.status(), StatusCode::OK);
    let validated = runner
        .captured_validated()
        .expect("captured validated request");
    assert!(!validated.prompt.contains("Readable scopes:\n"));
    assert!(!validated.system_prompt.contains("Readable scopes:\n"));
}

#[tokio::test]
async fn run_endpoint_rejects_control_characters_in_context_identifiers() {
    let runner = Arc::new(CapturingRunner::default());
    let app = build_test_app(runner);
    let mut request = base_request();
    request.platform_account_id = "qq:bot_uin:123\nadmin".into();
    let payload = serde_json::to_vec(&request).expect("serialize request");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("build request"),
        )
        .await
        .expect("run request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let json: Value = serde_json::from_slice(&body).expect("decode response");
    assert_eq!(json["error_code"], "invalid_request");
    assert_eq!(
        json["message"],
        "platform_account_id contains unsupported control characters or backticks"
    );
}

#[tokio::test]
async fn run_endpoint_keeps_runtime_context_without_pack_items() {
    let settings = test_settings();
    let store = HistoryStore::open(&settings.history_db_path).expect("history store");
    seed_history_message(&store, "hist-1", "之前讨论过的上下文");

    let runner = Arc::new(CapturingRunner::default());
    let app = build_test_app_with_settings(runner.clone(), settings);
    let payload = serde_json::to_vec(&base_request()).expect("serialize request");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("build request"),
        )
        .await
        .expect("run request");

    assert_eq!(response.status(), StatusCode::OK);
    let validated = runner
        .captured_validated()
        .expect("captured validated request");
    assert!(!validated.prompt.contains("Readable scopes:\n"));
    assert!(!validated.system_prompt.contains("Readable scopes:\n"));
}

#[tokio::test]
async fn qq_ingress_builds_trigger_context_with_message_ids_and_mention_refs() {
    let runner = Arc::new(CapturingRunner::default());
    let app = build_test_app_with_message_support(
        runner.clone(),
        Arc::new(NullMessenger),
        test_settings(),
    );
    let payload = serde_json::json!({
        "time": 1_710_000_000,
        "post_type": "message",
        "message_type": "group",
        "group_id": 5566,
        "user_id": 42,
        "message_id": 99,
        "raw_message": "[CQ:at,qq=123] 请你给 [CQ:at,qq=77] 发消息",
        "message": [
            {"type":"at","data":{"qq":"123"}},
            {"type":"text","data":{"text":" 请你给 "}},
            {"type":"at","data":{"qq":"77"}},
            {"type":"text","data":{"text":" 发消息"}}
        ]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/platforms/qq/napcat/events")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("build request"),
        )
        .await
        .expect("run request");

    assert_eq!(response.status(), StatusCode::OK);
    let captured_request = runner.captured_request().expect("captured request");
    let validated = runner
        .captured_validated()
        .expect("captured validated request");

    assert_eq!(captured_request.prompt, "@123 请你给 @77 发消息");
    assert_eq!(captured_request.trigger_message_id.as_deref(), Some("99"));
    assert_eq!(captured_request.trigger_reply_to_message_id, None);
    assert_eq!(captured_request.mention_refs.len(), 1);
    assert_eq!(captured_request.mention_refs[0].ref_id, "m1");
    assert_eq!(captured_request.mention_refs[0].actor_id, "qq:user:77");
    assert_eq!(captured_request.mention_refs[0].display, "@77");
    assert!(validated.prompt.contains("\"trigger_message_id\":\"99\""));
    assert!(validated.prompt.contains("@77[#m1]"));
    assert!(validated.prompt.contains("\"actor_id\":\"qq:user:77\""));
}
