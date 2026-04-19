use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_runner::models::{
    ErrorResponse, MessageRequest, MessageResponse, RunRequest, RunResponse,
};
use agent_runner::session_store::SessionStore;
use axum::{
    body::Body,
    body::to_bytes,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

fn base_request() -> RunRequest {
    RunRequest {
        platform: "qq".into(),
        platform_account_id: "qq:bot_uin:1".into(),
        conversation_id: "conv-1".into(),
        session_id: "qq-user-1".into(),
        user_id: "1".into(),
        chat_type: "private".into(),
        cwd: "/workspace".into(),
        prompt: "hello".into(),
        timeout_secs: None,
    }
}

fn base_message_request() -> MessageRequest {
    MessageRequest {
        session_id: "qq-user-1".into(),
        text: "hello from outbox".into(),
        reply_to_message_id: None,
        mention_user_id: None,
    }
}

fn test_settings() -> agent_runner::config::Settings {
    agent_runner::config::Settings {
        bind_addr: "127.0.0.1:8787".into(),
        default_timeout_secs: 120,
        max_timeout_secs: 300,
        container_name: "claude-runner".into(),
        image_name: "dogbot/claude-runner:local".into(),
        workspace_dir: "/tmp/agent-runner-tests/workdir".into(),
        state_dir: "/tmp/agent-runner-tests/state".into(),
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
        control_plane_db_path: "/tmp/agent-runner-tests/state/control.db".into(),
        admin_actor_ids: vec!["qq:user:1".into()],
        session_db_path: "/tmp/agent-runner-tests/state/runner.db".into(),
        container_cpu_cores: 4,
        container_memory_mb: 4096,
        container_disk_gb: 50,
        container_pids_limit: 256,
    }
}

#[test]
fn run_request_validation_returns_timeout_and_cwd() {
    let request = base_request();
    let validated = request.validate(120, 300).unwrap();
    assert_eq!(validated.timeout_secs, 120);
    assert_eq!(validated.cwd, "/workspace");
}

#[test]
fn run_request_rejects_timeout_over_max() {
    let mut request = base_request();
    request.timeout_secs = Some(500);

    let err = request.validate(120, 300).unwrap_err();
    assert!(err.contains("timeout exceeds configured max"));
}

#[test]
fn run_request_validation_accepts_exact_allowed_cwds() {
    for cwd in ["/workspace", "/state"] {
        let mut request = base_request();
        request.cwd = cwd.into();

        let validated = request.validate(120, 300).unwrap();
        assert_eq!(validated.cwd, cwd);
    }
}

#[test]
fn run_request_validation_rejects_disallowed_cwds() {
    for cwd in ["/workspace-evil", "/stateful", "/workspace/../etc"] {
        let mut request = base_request();
        request.cwd = cwd.into();

        let err = request.validate(120, 300).unwrap_err();
        assert!(err.contains(cwd), "error should mention {cwd}: {err}");
    }
}

#[test]
fn run_response_serializes_expected_fields() {
    let response = RunResponse {
        status: "ok".into(),
        stdout: "hello".into(),
        stderr: "".into(),
        exit_code: 0,
        timed_out: false,
        duration_ms: 123,
    };

    let json: Value = serde_json::to_value(&response).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["stdout"], "hello");
    assert_eq!(json["exit_code"], 0);
    assert_eq!(json["duration_ms"], 123);
}

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let app = agent_runner::server::build_test_app(Arc::new(MockRunner::success()));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn run_endpoint_returns_success_body() {
    let app = agent_runner::server::build_test_app(Arc::new(MockRunner::success()));
    let payload = serde_json::to_vec(&base_request()).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn run_endpoint_returns_bad_request_for_invalid_cwd() {
    let app = agent_runner::server::build_test_app(Arc::new(MockRunner::success()));
    let mut request = base_request();
    request.cwd = "/workspace/child".into();
    let payload = serde_json::to_vec(&request).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn run_endpoint_returns_timeout_status_for_runner_timeout() {
    let app = agent_runner::server::build_test_app(Arc::new(MockRunner::timeout()));
    let payload = serde_json::to_vec(&base_request()).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
}

#[tokio::test]
async fn run_endpoint_normalizes_invalid_json_errors() {
    let app = agent_runner::server::build_test_app(Arc::new(MockRunner::success()));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from("{not-json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error_code"], "invalid_json");
}

#[tokio::test]
async fn run_endpoint_queues_one_waiting_request_before_overflowing() {
    let settings = test_settings();
    let app = agent_runner::server::build_test_app_with_settings(
        Arc::new(MockRunner::sleeping()),
        settings,
    );
    let payload = serde_json::to_vec(&base_request()).unwrap();

    let first_app = app.clone();
    let first_payload = payload.clone();
    let first = tokio::spawn(async move {
        first_app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(first_payload))
                    .unwrap(),
            )
            .await
            .unwrap()
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let second_app = app.clone();
    let second_payload = payload.clone();
    let second = tokio::spawn(async move {
        second_app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(second_payload))
                    .unwrap(),
            )
            .await
            .unwrap()
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(first.await.unwrap().status(), StatusCode::OK);
    assert_eq!(second.await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn run_endpoint_rate_limits_after_global_budget_is_exhausted() {
    let mut settings = test_settings();
    settings.max_concurrent_runs = 2;
    settings.max_queue_depth = 2;
    settings.global_rate_limit_per_minute = 1;
    settings.user_rate_limit_per_minute = 10;
    settings.conversation_rate_limit_per_minute = 10;
    let app = agent_runner::server::build_test_app_with_settings(
        Arc::new(MockRunner::success()),
        settings,
    );
    let payload = serde_json::to_vec(&base_request()).unwrap();

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = to_bytes(second.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error_code"], "rate_limited");
}

#[tokio::test]
async fn run_endpoint_rate_limits_per_user() {
    let mut settings = test_settings();
    settings.max_concurrent_runs = 2;
    settings.max_queue_depth = 2;
    settings.global_rate_limit_per_minute = 10;
    settings.user_rate_limit_per_minute = 1;
    settings.conversation_rate_limit_per_minute = 10;
    let app = agent_runner::server::build_test_app_with_settings(
        Arc::new(MockRunner::success()),
        settings,
    );
    let payload = serde_json::to_vec(&base_request()).unwrap();

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn run_endpoint_rate_limits_per_conversation() {
    let mut settings = test_settings();
    settings.max_concurrent_runs = 2;
    settings.max_queue_depth = 2;
    settings.global_rate_limit_per_minute = 10;
    settings.user_rate_limit_per_minute = 10;
    settings.conversation_rate_limit_per_minute = 1;
    let app = agent_runner::server::build_test_app_with_settings(
        Arc::new(MockRunner::success()),
        settings,
    );
    let payload = serde_json::to_vec(&base_request()).unwrap();

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn message_endpoint_sends_to_existing_session() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("runner.db");
    let store = SessionStore::open(&db_path).unwrap();
    store
        .get_or_create_session("qq-user-1", "qq", "qq:private:1", "1")
        .unwrap();

    let messenger = Arc::new(MockMessenger::success());
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner::success()),
        messenger.clone(),
        test_settings_with_session_db(&db_path),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&base_message_request()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        messenger.last_request().unwrap(),
        MessageRequest {
            session_id: "qq-user-1".into(),
            text: "hello from outbox".into(),
            reply_to_message_id: None,
            mention_user_id: None,
        }
    );
}

#[tokio::test]
async fn message_endpoint_returns_not_found_for_unknown_session() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("runner.db");
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner::success()),
        Arc::new(MockMessenger::success()),
        test_settings_with_session_db(&db_path),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&base_message_request()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn message_endpoint_passes_reply_and_mention_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("runner.db");
    let store = SessionStore::open(&db_path).unwrap();
    store
        .get_or_create_session("qq-user-1", "qq", "qq:group:100", "1")
        .unwrap();

    let messenger = Arc::new(MockMessenger::success());
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner::success()),
        messenger.clone(),
        test_settings_with_session_db(&db_path),
    );

    let mut request = base_message_request();
    request.reply_to_message_id = Some("42".into());
    request.mention_user_id = Some("99".into());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let sent = messenger.last_request().unwrap();
    assert_eq!(sent.reply_to_message_id.as_deref(), Some("42"));
    assert_eq!(sent.mention_user_id.as_deref(), Some("99"));
}

#[tokio::test]
async fn message_endpoint_defaults_group_mention_to_session_user() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("runner.db");
    let store = SessionStore::open(&db_path).unwrap();
    store
        .get_or_create_session("qq-group-user-1", "qq", "qq:group:100", "88")
        .unwrap();

    let messenger = Arc::new(MockMessenger::success());
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner::success()),
        messenger.clone(),
        test_settings_with_session_db(&db_path),
    );

    let request = MessageRequest {
        session_id: "qq-group-user-1".into(),
        text: "follow-up".into(),
        reply_to_message_id: None,
        mention_user_id: None,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let sent = messenger.last_request().unwrap();
    assert_eq!(sent.mention_user_id.as_deref(), Some("88"));
}

struct MockRunner {
    outcome: Result<RunResponse, ErrorResponse>,
}

impl MockRunner {
    fn success() -> Self {
        Self {
            outcome: Ok(RunResponse {
                status: "ok".into(),
                stdout: "done".into(),
                stderr: String::new(),
                exit_code: 0,
                timed_out: false,
                duration_ms: 1,
            }),
        }
    }

    fn timeout() -> Self {
        Self {
            outcome: Err(ErrorResponse {
                status: "error".into(),
                error_code: "timeout".into(),
                message: "command exceeded timeout".into(),
                timed_out: true,
            }),
        }
    }

    fn sleeping() -> Self {
        Self {
            outcome: Ok(RunResponse {
                status: "ok".into(),
                stdout: "slow".into(),
                stderr: String::new(),
                exit_code: 0,
                timed_out: false,
                duration_ms: 1,
            }),
        }
    }
}

#[derive(Default)]
struct MockMessenger {
    sent: Mutex<Vec<MessageRequest>>,
}

impl MockMessenger {
    fn success() -> Self {
        Self::default()
    }

    fn last_request(&self) -> Option<MessageRequest> {
        self.sent.lock().unwrap().last().cloned()
    }
}

#[async_trait::async_trait]
impl agent_runner::server::Runner for MockRunner {
    async fn run(
        &self,
        _request: RunRequest,
        _validated: agent_runner::models::ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse> {
        if self.outcome.as_ref().ok().map(|resp| resp.stdout.as_str()) == Some("slow") {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        self.outcome.clone()
    }
}

#[async_trait::async_trait]
impl agent_runner::server::Messenger for MockMessenger {
    async fn send(
        &self,
        request: MessageRequest,
        _session: agent_runner::session_store::SessionRecord,
    ) -> Result<MessageResponse, ErrorResponse> {
        self.sent.lock().unwrap().push(request);
        Ok(MessageResponse {
            status: "ok".into(),
            message_id: Some("msg-1".into()),
        })
    }
}

fn test_settings_with_session_db(db_path: &std::path::Path) -> agent_runner::config::Settings {
    let mut settings = test_settings();
    settings.session_db_path = db_path.display().to_string();
    settings.control_plane_db_path = db_path.with_file_name("control.db").display().to_string();
    settings
}
