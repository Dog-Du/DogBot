use std::sync::{Arc, Mutex};

use agent_runner::models::{ErrorResponse, RunRequest, RunResponse, ValidatedRunRequest};
use agent_runner::server::{Runner, build_test_app};
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
    prompt: Arc<Mutex<Option<String>>>,
    request: Arc<Mutex<Option<RunRequest>>>,
}

impl CapturingRunner {
    fn captured_prompt(&self) -> Option<String> {
        self.prompt.lock().expect("lock prompt").clone()
    }

    fn captured_request(&self) -> Option<RunRequest> {
        self.request.lock().expect("lock request").clone()
    }
}

#[async_trait]
impl Runner for CapturingRunner {
    async fn run(
        &self,
        request: RunRequest,
        _validated: ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse> {
        *self.prompt.lock().expect("lock prompt") = Some(request.prompt.clone());
        *self.request.lock().expect("lock request") = Some(request);
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
        timeout_secs: None,
    }
}

#[tokio::test]
async fn run_endpoint_prepends_readable_scopes_context_pack() {
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
    let captured = runner.captured_prompt().expect("captured prompt");
    assert!(captured.starts_with("Readable scopes:\n"));
    assert!(captured.contains("- user-private: qq:user:1"));
    assert!(captured.contains("qq:user:1"));
    assert!(captured.contains("qq:private:1"));
    assert!(captured.ends_with("hello"));
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
