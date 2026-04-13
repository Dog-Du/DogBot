use agent_runner::models::{RunRequest, RunResponse};
use serde_json::Value;

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
