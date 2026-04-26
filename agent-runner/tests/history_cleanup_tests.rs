use std::{collections::HashMap, sync::Arc};

use agent_runner::{
    config::Settings,
    history::{cleanup::purge_expired_history, store::HistoryStore},
    models::{
        ErrorResponse, MessageRequest, MessageResponse, RunRequest, RunResponse,
        ValidatedRunRequest,
    },
};
use axum::{
    body::Body,
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

fn test_settings(root: &std::path::Path) -> Settings {
    let mut settings = Settings::from_env_map(HashMap::new()).expect("default settings");
    settings.workspace_dir = root.join("workdir").display().to_string();
    settings.state_dir = root.join("state").display().to_string();
    settings.session_db_path = root.join("state/runner.db").display().to_string();
    settings.history_db_path = root.join("state/history.db").display().to_string();
    settings
}

#[test]
fn cleanup_removes_expired_messages_but_keeps_live_assets() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("history.db");
    let store = HistoryStore::open(&db_path).unwrap();

    store
        .insert_expired_message_for_test("m1", "qq:group:100")
        .unwrap();
    store
        .insert_live_asset_for_test("asset-1", "/tmp/a.png")
        .unwrap();

    purge_expired_history(&store).unwrap();

    assert_eq!(
        store.message_count("test:history", "qq:group:100").unwrap(),
        0
    );
    assert_eq!(store.asset_count().unwrap(), 1);
}

#[tokio::test]
async fn inbound_request_triggers_runtime_history_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let settings = test_settings(temp.path());
    let store = HistoryStore::open(&settings.history_db_path).unwrap();
    store
        .insert_expired_message_for_test("expired-1", "qq:group:cleanup")
        .unwrap();

    let app = agent_runner::server::build_test_app_with_message_support(
        Arc::new(MockRunner),
        Arc::new(NoopMessenger),
        settings.clone(),
    );
    let payload = serde_json::to_vec(&serde_json::json!({
        "time": 2,
        "post_type": "message",
        "message_type": "group",
        "group_id": 123,
        "user_id": 9,
        "message_id": "trigger-1",
        "raw_message": "[CQ:at,qq=123] summarize",
        "message": [
            {"type":"at","data":{"qq":"123"}},
            {"type":"text","data":{"text":" summarize"}}
        ]
    }))
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/platforms/qq/napcat/events")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let verifier = HistoryStore::open(&settings.history_db_path).unwrap();
    assert_eq!(
        verifier
            .message_count("test:history", "qq:group:cleanup")
            .unwrap(),
        0
    );
}
