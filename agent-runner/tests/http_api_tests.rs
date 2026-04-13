use agent_runner::models::RunRequest;

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
