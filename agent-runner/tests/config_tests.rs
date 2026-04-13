use agent_runner::config::Settings;

#[test]
fn settings_use_expected_defaults() {
    let settings = Settings::from_env_map(std::collections::HashMap::new()).unwrap();

    assert_eq!(settings.bind_addr, "127.0.0.1:8787");
    assert_eq!(settings.default_timeout_secs, 120);
    assert_eq!(settings.max_timeout_secs, 300);
    assert_eq!(settings.container_name, "claude-runner");
    assert_eq!(settings.image_name, "myqqbot/claude-runner:local");
    assert_eq!(settings.workspace_dir, "/srv/agent-workdir");
    assert_eq!(settings.state_dir, "/srv/agent-state");
    assert_eq!(
        settings.anthropic_base_url,
        "http://host.docker.internal:9000"
    );
    assert_eq!(settings.napcat_api_base_url, "http://127.0.0.1:3001");
    assert_eq!(settings.napcat_access_token, None);
    assert_eq!(settings.max_concurrent_runs, 10);
    assert_eq!(settings.max_queue_depth, 20);
    assert_eq!(settings.global_rate_limit_per_minute, 10);
    assert_eq!(settings.user_rate_limit_per_minute, 3);
    assert_eq!(settings.conversation_rate_limit_per_minute, 5);
    assert_eq!(settings.session_db_path, "/srv/agent-state/runner.db");
}

#[test]
fn settings_reject_default_timeout_above_max() {
    let env = std::collections::HashMap::from([
        ("DEFAULT_TIMEOUT_SECS".to_string(), "400".to_string()),
        ("MAX_TIMEOUT_SECS".to_string(), "300".to_string()),
    ]);

    let err = Settings::from_env_map(env).unwrap_err();
    assert!(
        err.to_string()
            .contains("DEFAULT_TIMEOUT_SECS must be <= MAX_TIMEOUT_SECS")
    );
}

#[test]
fn settings_use_claude_container_name_override() {
    let env = std::collections::HashMap::from([(
        "CLAUDE_CONTAINER_NAME".to_string(),
        "custom-claude".to_string(),
    )]);

    let settings = Settings::from_env_map(env).unwrap();
    assert_eq!(settings.container_name, "custom-claude");
}

#[test]
fn settings_treat_empty_napcat_token_as_absent() {
    let env =
        std::collections::HashMap::from([("NAPCAT_ACCESS_TOKEN".to_string(), "   ".to_string())]);

    let settings = Settings::from_env_map(env).unwrap();
    assert_eq!(settings.napcat_access_token, None);
}

#[test]
fn settings_allow_runner_limit_overrides() {
    let env = std::collections::HashMap::from([
        (
            "CLAUDE_IMAGE_NAME".to_string(),
            "custom/claude:1".to_string(),
        ),
        ("AGENT_WORKSPACE_DIR".to_string(), "/tmp/work".to_string()),
        ("AGENT_STATE_DIR".to_string(), "/tmp/state".to_string()),
        (
            "ANTHROPIC_BASE_URL".to_string(),
            "http://proxy.internal:9000".to_string(),
        ),
        (
            "NAPCAT_API_BASE_URL".to_string(),
            "http://127.0.0.1:3100".to_string(),
        ),
        (
            "NAPCAT_ACCESS_TOKEN".to_string(),
            "secret-token".to_string(),
        ),
        ("MAX_CONCURRENT_RUNS".to_string(), "4".to_string()),
        ("MAX_QUEUE_DEPTH".to_string(), "9".to_string()),
        ("GLOBAL_RATE_LIMIT_PER_MINUTE".to_string(), "15".to_string()),
        ("USER_RATE_LIMIT_PER_MINUTE".to_string(), "6".to_string()),
        (
            "CONVERSATION_RATE_LIMIT_PER_MINUTE".to_string(),
            "7".to_string(),
        ),
        (
            "SESSION_DB_PATH".to_string(),
            "/tmp/state/runner.db".to_string(),
        ),
    ]);

    let settings = Settings::from_env_map(env).unwrap();
    assert_eq!(settings.image_name, "custom/claude:1");
    assert_eq!(settings.workspace_dir, "/tmp/work");
    assert_eq!(settings.state_dir, "/tmp/state");
    assert_eq!(settings.anthropic_base_url, "http://proxy.internal:9000");
    assert_eq!(settings.napcat_api_base_url, "http://127.0.0.1:3100");
    assert_eq!(
        settings.napcat_access_token.as_deref(),
        Some("secret-token")
    );
    assert_eq!(settings.max_concurrent_runs, 4);
    assert_eq!(settings.max_queue_depth, 9);
    assert_eq!(settings.global_rate_limit_per_minute, 15);
    assert_eq!(settings.user_rate_limit_per_minute, 6);
    assert_eq!(settings.conversation_rate_limit_per_minute, 7);
    assert_eq!(settings.session_db_path, "/tmp/state/runner.db");
}
