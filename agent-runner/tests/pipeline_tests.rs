use std::sync::{Arc, Mutex};

use agent_runner::models::{
    ErrorResponse, MessageRequest, MessageResponse, RunRequest, RunResponse, ValidatedRunRequest,
};
use agent_runner::protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart};
use agent_runner::server::{Messenger, Runner, build_test_app_with_message_support};
use agent_runner::trigger_resolver::should_trigger_run;
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[derive(Default)]
struct CapturingRunner {
    request: Arc<Mutex<Option<RunRequest>>>,
}

impl CapturingRunner {
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

#[derive(Default)]
struct NoopMessenger;

#[async_trait]
impl Messenger for NoopMessenger {
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

#[test]
fn group_message_requires_structured_bot_mention() {
    let event = CanonicalEvent {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation: "qq:group:5566".into(),
        actor: "qq:user:42".into(),
        event_id: "evt-1".into(),
        timestamp_epoch_secs: 1,
        kind: EventKind::Message {
            message: CanonicalMessage {
                message_id: "msg-1".into(),
                reply_to: None,
                parts: vec![MessagePart::Text {
                    text: "hello".into(),
                }],
                mentions: vec![],
                native_metadata: serde_json::json!({}),
            },
        },
        raw_native_payload: serde_json::json!({}),
    };

    assert!(!should_trigger_run(&event));
}

#[test]
fn private_non_empty_text_triggers_run() {
    let event = CanonicalEvent {
        platform: "wechatpadpro".into(),
        platform_account: "wechatpadpro:account:bot".into(),
        conversation: "wechatpadpro:private:wxid_user".into(),
        actor: "wechatpadpro:user:wxid_user".into(),
        event_id: "evt-2".into(),
        timestamp_epoch_secs: 1,
        kind: EventKind::Message {
            message: CanonicalMessage {
                message_id: "msg-2".into(),
                reply_to: None,
                parts: vec![MessagePart::Text {
                    text: "帮我总结一下".into(),
                }],
                mentions: vec![],
                native_metadata: serde_json::json!({}),
            },
        },
        raw_native_payload: serde_json::json!({}),
    };

    assert!(should_trigger_run(&event));
}

#[tokio::test]
async fn wechat_webhook_private_text_enters_run_pipeline() {
    let runner = Arc::new(CapturingRunner::default());
    let temp_state_dir = std::env::temp_dir().join(format!(
        "agent-runner-pipeline-tests-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let app = build_test_app_with_message_support(runner.clone(), Arc::new(NoopMessenger), {
        let mut settings =
            agent_runner::config::Settings::from_env_map(std::collections::HashMap::new())
                .expect("default settings");
        settings.workspace_dir = temp_state_dir.join("workdir").display().to_string();
        settings.state_dir = temp_state_dir.join("state").display().to_string();
        settings.session_db_path = temp_state_dir.join("state/runner.db").display().to_string();
        settings.history_db_path = temp_state_dir
            .join("state/history.db")
            .display()
            .to_string();
        settings.platform_wechatpadpro_account_id = Some("wechatpadpro:account:bot".into());
        settings
    });
    let payload = serde_json::json!({
        "message": {
            "msgId": "123",
            "senderWxid": "wxid_user",
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
    let request = runner.captured_request().expect("captured request");
    assert_eq!(request.platform, "wechatpadpro");
    assert_eq!(request.chat_type, "private");
    assert_eq!(request.prompt, "帮我总结一下");
}
