use std::sync::{Arc, Mutex};

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
        trigger_summary: Some("hello".into()),
        trigger_message_id: None,
        trigger_reply_to_message_id: None,
        mention_refs: Vec::new(),
        reply_excerpt: None,
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
    let root = std::env::temp_dir().join(format!(
        "agent-runner-http-tests-{}-{}",
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
        platform_qq_account_id: None,
        platform_qq_bot_id: None,
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

#[test]
fn run_request_validation_returns_cwd() {
    let request = base_request();
    let validated = request.validate().unwrap();
    assert_eq!(validated.cwd, "/workspace");
}

#[test]
fn run_request_validation_ignores_timeout_secs() {
    let mut request = base_request();
    request.timeout_secs = Some(500_000);

    let validated = request.validate().unwrap();
    assert_eq!(validated.cwd, "/workspace");
}

#[test]
fn run_request_validation_accepts_exact_allowed_cwds() {
    for cwd in ["/workspace", "/state"] {
        let mut request = base_request();
        request.cwd = cwd.into();

        let validated = request.validate().unwrap();
        assert_eq!(validated.cwd, cwd);
    }
}

#[test]
fn run_request_validation_rejects_disallowed_cwds() {
    for cwd in ["/workspace-evil", "/stateful", "/workspace/../etc"] {
        let mut request = base_request();
        request.cwd = cwd.into();

        let err = request.validate().unwrap_err();
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
async fn app_exposes_platform_ingress_routes() {
    let app = agent_runner::server::build_test_app(Arc::new(MockRunner::success()));

    let wechat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/v1/platforms/wechatpadpro/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wechat.status(), StatusCode::OK);

    let qq = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/platforms/qq/napcat/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(qq.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn inbound_messages_route_is_removed() {
    let app = agent_runner::server::build_test_app(Arc::new(MockRunner::success()));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/inbound-messages")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
async fn message_endpoint_sends_to_existing_session() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };
    let store = SessionStore::open(&scope.settings).unwrap();
    store.initialize_schema().unwrap();
    let session_id = scope.external("qq-user-1");
    let account = scope.account("qq:bot_uin:123");
    store
        .get_or_create_bound_session(&session_id, "qq", &account, "qq:private:1")
        .unwrap();

    let messenger = Arc::new(MockMessenger::success());
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner::success()),
        messenger.clone(),
        scope.settings,
    );
    let mut request = base_message_request();
    request.session_id = session_id.clone();

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
    assert_eq!(
        messenger.last_request().unwrap(),
        MessageRequest {
            session_id,
            text: "hello from outbox".into(),
            reply_to_message_id: None,
            mention_user_id: None,
        }
    );
}

#[tokio::test]
async fn message_endpoint_trims_session_id_before_lookup() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };
    let store = SessionStore::open(&scope.settings).unwrap();
    store.initialize_schema().unwrap();
    let session_id = scope.external("qq-user-1");
    let account = scope.account("qq:bot_uin:123");
    store
        .get_or_create_bound_session(&session_id, "qq", &account, "qq:private:1")
        .unwrap();

    let messenger = Arc::new(MockMessenger::success());
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner::success()),
        messenger.clone(),
        scope.settings,
    );

    let mut request = base_message_request();
    request.session_id = format!("  {session_id}  ");

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
    assert_eq!(messenger.last_request().unwrap().text, "hello from outbox");
}

#[tokio::test]
async fn message_endpoint_returns_not_found_for_unknown_session() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };
    let store = SessionStore::open(&scope.settings).unwrap();
    store.initialize_schema().unwrap();
    let unknown_session_id = scope.external("unknown-session");
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner::success()),
        Arc::new(MockMessenger::success()),
        scope.settings,
    );

    let mut request = base_message_request();
    request.session_id = unknown_session_id;

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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn message_endpoint_passes_reply_and_mention_metadata() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };
    let store = SessionStore::open(&scope.settings).unwrap();
    store.initialize_schema().unwrap();
    let session_id = scope.external("qq-user-1");
    let account = scope.account("qq:bot_uin:123");
    store
        .get_or_create_bound_session(&session_id, "qq", &account, "qq:group:100")
        .unwrap();

    let messenger = Arc::new(MockMessenger::success());
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner::success()),
        messenger.clone(),
        scope.settings,
    );

    let mut request = base_message_request();
    request.session_id = session_id;
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
async fn message_endpoint_does_not_infer_group_mention_from_conversation_session() {
    let Some(scope) = PostgresSessionScope::new() else {
        return;
    };
    let store = SessionStore::open(&scope.settings).unwrap();
    store.initialize_schema().unwrap();
    let session_id = scope.external("qq-group-user-1");
    let account = scope.account("qq:bot_uin:123");
    store
        .get_or_create_bound_session(&session_id, "qq", &account, "qq:group:100")
        .unwrap();

    let messenger = Arc::new(MockMessenger::success());
    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner::success()),
        messenger.clone(),
        scope.settings,
    );

    let request = MessageRequest {
        session_id,
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
    assert_eq!(sent.mention_user_id, None);
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

struct PostgresSessionScope {
    settings: agent_runner::config::Settings,
    suffix: String,
}

impl PostgresSessionScope {
    fn new() -> Option<Self> {
        let Some(database_url) = std::env::var("DOGBOT_TEST_DATABASE_URL").ok() else {
            eprintln!("DOGBOT_TEST_DATABASE_URL unset; skipping postgres integration test");
            return None;
        };
        let mut settings = test_settings();
        settings.database_url = database_url;
        Some(Self {
            settings,
            suffix: uuid::Uuid::new_v4().to_string(),
        })
    }

    fn external(&self, value: &str) -> String {
        format!("{value}:{}", self.suffix)
    }

    fn account(&self, value: &str) -> String {
        format!("{value}:test:{}", self.suffix)
    }
}
