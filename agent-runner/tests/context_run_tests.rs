use std::sync::{Arc, Mutex};

use agent_runner::config::Settings;
use agent_runner::history::store::HistoryStore;
use agent_runner::models::{ErrorResponse, RunRequest, RunResponse, ValidatedRunRequest};
use agent_runner::server::{Runner, build_test_app, build_test_app_with_settings};
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
        history_db_path: root.join("state/history.db").display().to_string(),
        container_cpu_cores: 4,
        container_memory_mb: 4096,
        container_disk_gb: 50,
        container_pids_limit: 256,
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
async fn run_endpoint_appends_history_evidence_pack_when_history_exists() {
    let settings = test_settings();
    let store = HistoryStore::open(&settings.history_db_path).expect("history store");
    store
        .insert_message(
            "hist-1",
            "qq:private:1",
            "qq:user:1",
            "之前讨论过的上下文",
            1,
        )
        .expect("seed history");

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
    let captured = runner.captured_prompt().expect("captured prompt");
    assert!(captured.contains("History evidence for qq:private:1"));
    assert!(captured.contains("Recent context"));
    assert!(captured.contains("之前讨论过的上下文"));
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
async fn run_endpoint_includes_enabled_pack_items_in_context() {
    let settings = test_settings();
    let pack_dir = std::path::Path::new(&settings.content_root).join("packs/base");
    std::fs::create_dir_all(&pack_dir).expect("create pack dir");
    std::fs::write(
        pack_dir.join("manifest.json"),
        r#"{
            "pack_id":"base",
            "version":1,
            "title":"DogBot Base Pack",
            "kind":"resource-pack",
            "source":{"source_id":"dogbot_local","repo_url":"local","ref":"workspace","license":"Proprietary"},
            "items":[
                {
                    "id":"base.system",
                    "kind":"prompt",
                    "path":"prompts/system.md",
                    "title":"System Prompt",
                    "summary":"base prompt",
                    "tags":["base"],
                    "enabled_by_default":true,
                    "platform_overrides":[],
                    "upstream_path":""
                }
            ]
        }"#,
    )
    .expect("write manifest");

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
    let captured = runner.captured_prompt().expect("captured prompt");
    assert!(captured.contains("Enabled pack items:"));
    assert!(captured.contains("base.system"));
}
