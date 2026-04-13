use agent_runner::models::{RunRequest, RunResponse};
use serde_json::Value;

#[test]
fn run_request_uses_default_timeout_when_missing() {
    let request = RunRequest {
        platform: "qq".into(),
        conversation_id: "conv-1".into(),
        session_id: "qq-user-1".into(),
        user_id: "1".into(),
        chat_type: "private".into(),
        cwd: "/workspace".into(),
        prompt: "hello".into(),
        timeout_secs: None,
    };

    assert_eq!(request.effective_timeout(120, 300).unwrap(), 120);
}

#[test]
fn run_request_rejects_timeout_over_max() {
    let request = RunRequest {
        platform: "qq".into(),
        conversation_id: "conv-1".into(),
        session_id: "qq-user-1".into(),
        user_id: "1".into(),
        chat_type: "private".into(),
        cwd: "/workspace".into(),
        prompt: "hello".into(),
        timeout_secs: Some(500),
    };

    let err = request.effective_timeout(120, 300).unwrap_err();
    assert!(err.contains("timeout exceeds configured max"));
}

#[test]
fn run_request_validates_cwd_prefix() {
    let mut request = RunRequest {
        platform: "qq".into(),
        conversation_id: "conv-1".into(),
        session_id: "qq-user-1".into(),
        user_id: "1".into(),
        chat_type: "private".into(),
        cwd: "/workspace".into(),
        prompt: "hello".into(),
        timeout_secs: None,
    };

    assert!(request.validate_cwd().is_ok());

    request.cwd = "/etc".into();
    let err = request.validate_cwd().unwrap_err();
    assert!(err.contains("cwd"));
    assert!(err.contains("/workspace"));
    assert!(err.contains("/state"));
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
