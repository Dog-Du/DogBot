use std::sync::Arc;
use std::time::Duration;

use agent_runner::models::{ErrorResponse, RunRequest, RunResponse};
use axum::{
    body::to_bytes,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

fn base_request() -> RunRequest {
    RunRequest {
        platform: "qq".into(),
        conversation_id: "conv-1".into(),
        session_id: "qq-user-1".into(),
        user_id: "1".into(),
        chat_type: "private".into(),
        cwd: "/workspace".into(),
        prompt: "hello".into(),
        timeout_secs: None,
    }
}

fn test_settings() -> agent_runner::config::Settings {
    agent_runner::config::Settings {
        bind_addr: "127.0.0.1:8787".into(),
        default_timeout_secs: 120,
        max_timeout_secs: 300,
        container_name: "claude-runner".into(),
        image_name: "myqqbot/claude-runner:local".into(),
        workspace_dir: "/srv/agent-workdir".into(),
        state_dir: "/srv/agent-state".into(),
        anthropic_base_url: "http://host.docker.internal:9000".into(),
        max_concurrent_runs: 1,
        max_queue_depth: 1,
        global_rate_limit_per_minute: 10,
        user_rate_limit_per_minute: 3,
        conversation_rate_limit_per_minute: 5,
        session_db_path: "/srv/agent-state/runner.db".into(),
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
    let app =
        agent_runner::server::build_test_app_with_settings(Arc::new(MockRunner::success()), settings);
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
    let app =
        agent_runner::server::build_test_app_with_settings(Arc::new(MockRunner::success()), settings);
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
    let app =
        agent_runner::server::build_test_app_with_settings(Arc::new(MockRunner::success()), settings);
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
