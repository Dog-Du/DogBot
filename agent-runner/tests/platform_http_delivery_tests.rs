use std::sync::{Arc, Mutex};

use agent_runner::models::{ErrorResponse, RunRequest, RunResponse, ValidatedRunRequest};
use agent_runner::server::{Runner, build_test_app_with_settings};
use async_trait::async_trait;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::post,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tower::ServiceExt;

#[derive(Clone, Default)]
struct Capture {
    requests: Arc<Mutex<Vec<(String, Value)>>>,
}

impl Capture {
    fn push(&self, path: String, body: Value) {
        self.requests.lock().unwrap().push((path, body));
    }

    fn last(&self) -> Option<(String, Value)> {
        self.requests.lock().unwrap().last().cloned()
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

fn test_settings() -> agent_runner::config::Settings {
    let root = std::env::temp_dir().join(format!(
        "agent-runner-platform-http-tests-{}-{}",
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

async fn spawn_mock_server(capture: Capture, response: Value) -> String {
    let app = Router::new()
        .route(
            "/message/SendTextMessage",
            post({
                let capture = capture.clone();
                let response = response.clone();
                move |request: Request<Body>| {
                    let capture = capture.clone();
                    let response = response.clone();
                    async move {
                        let path = request
                            .uri()
                            .path_and_query()
                            .map(|value| value.as_str().to_string())
                            .unwrap_or_else(|| "/".to_string());
                        let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
                        let json: Value = serde_json::from_slice(&body).unwrap();
                        capture.push(path, json);
                        axum::Json(response)
                    }
                }
            }),
        )
        .route(
            "/send_group_msg",
            post({
                let capture = capture.clone();
                let response = response.clone();
                move |request: Request<Body>| {
                    let capture = capture.clone();
                    let response = response.clone();
                    async move {
                        let path = request
                            .uri()
                            .path_and_query()
                            .map(|value| value.as_str().to_string())
                            .unwrap_or_else(|| "/".to_string());
                        let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
                        let json: Value = serde_json::from_slice(&body).unwrap();
                        capture.push(path, json);
                        axum::Json(response)
                    }
                }
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn wechat_message_endpoint_uses_registered_platform_adapter() {
    let capture = Capture::default();
    let base_url = spawn_mock_server(capture.clone(), json!({"Code": 200, "Data": {"MsgId": "wx-out-1"}})).await;
    let mut settings = test_settings();
    settings.wechatpadpro_base_url = base_url;
    settings.wechatpadpro_account_key = Some("test-key".into());

    let store = agent_runner::session_store::SessionStore::open(&settings.session_db_path).unwrap();
    store
        .get_or_create_bound_session(
            "wechat-session-1",
            "wechatpadpro",
            "wechatpadpro:account:bot",
            "wechatpadpro:private:wxid_user_1",
        )
        .unwrap();

    let app = build_test_app_with_settings(Arc::new(SuccessRunner), settings);
    let request = json!({
        "session_id": "wechat-session-1",
        "text": "hello from outbox",
        "reply_to_message_id": null,
        "mention_user_id": null
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let (path, payload) = capture.last().expect("captured request");
    assert_eq!(path, "/message/SendTextMessage?key=test-key");
    assert_eq!(payload["MsgItem"][0]["ToUserName"], "wxid_user_1");
    assert_eq!(payload["MsgItem"][0]["TextContent"], "hello from outbox");
}

#[tokio::test]
async fn wechat_ingress_uses_registered_platform_adapter_for_delivery() {
    let capture = Capture::default();
    let base_url = spawn_mock_server(capture.clone(), json!({"Code": 200, "Data": {"MsgId": "wx-out-2"}})).await;
    let mut settings = test_settings();
    settings.wechatpadpro_base_url = base_url;
    settings.wechatpadpro_account_key = Some("test-key".into());

    let app = build_test_app_with_settings(Arc::new(SuccessRunner), settings);
    let payload = json!({
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
    let (path, body) = capture.last().expect("captured request");
    assert_eq!(path, "/message/SendTextMessage?key=test-key");
    assert_eq!(body["MsgItem"][0]["ToUserName"], "wxid_user_1");
    assert_eq!(body["MsgItem"][0]["TextContent"], "收到");
}

#[tokio::test]
async fn qq_ingress_group_message_uses_registered_platform_adapter_for_delivery() {
    let capture = Capture::default();
    let base_url =
        spawn_mock_server(capture.clone(), json!({"status": "ok", "data": {"message_id": 91}}))
            .await;
    let mut settings = test_settings();
    settings.napcat_api_base_url = base_url;

    let app = build_test_app_with_settings(Arc::new(SuccessRunner), settings);
    let payload = json!({
        "time": 1_710_000_000,
        "post_type": "message",
        "message_type": "group",
        "group_id": 5566,
        "user_id": 42,
        "message_id": 99,
        "raw_message": "[CQ:at,qq=123] hello",
        "message": [
            {"type":"at","data":{"qq":"123"}},
            {"type":"text","data":{"text":" hello"}}
        ]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/platforms/qq/napcat/ws")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let (path, body) = capture.last().expect("captured request");
    assert_eq!(path, "/send_group_msg");
    assert_eq!(body["group_id"], 5566);
    assert_eq!(body["message"], "[CQ:reply,id=99][CQ:at,qq=42] 收到");
}
