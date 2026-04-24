use agent_runner::config::Settings;

#[test]
fn settings_use_expected_defaults() {
    let settings = Settings::from_env_map(std::collections::HashMap::new()).unwrap();

    assert_eq!(settings.bind_addr, "127.0.0.1:8787");
    assert_eq!(settings.default_timeout_secs, 120);
    assert_eq!(settings.max_timeout_secs, 300);
    assert_eq!(settings.container_name, "claude-runner");
    assert_eq!(settings.image_name, "dogbot/claude-runner:local");
    assert_eq!(settings.workspace_dir, "/srv/agent-workdir");
    assert_eq!(settings.state_dir, "/srv/agent-state");
    assert_eq!(
        settings.anthropic_base_url,
        "http://127.0.0.1:8080/anthropic"
    );
    assert_eq!(settings.anthropic_api_key, "dummy");
    assert_eq!(settings.bifrost_port, 8080);
    assert_eq!(settings.bifrost_provider_name, "primary");
    assert_eq!(settings.bifrost_model, "primary/model-id");
    assert_eq!(
        settings.bifrost_upstream_base_url,
        "https://example.com"
    );
    assert_eq!(settings.bifrost_upstream_api_key, "replace-me");
    assert_eq!(settings.bifrost_upstream_provider_type, "openai");
    assert_eq!(settings.napcat_api_base_url, "http://127.0.0.1:3001");
    assert_eq!(settings.napcat_access_token, None);
    assert_eq!(settings.max_concurrent_runs, 10);
    assert_eq!(settings.max_queue_depth, 20);
    assert_eq!(settings.global_rate_limit_per_minute, 10);
    assert_eq!(settings.user_rate_limit_per_minute, 3);
    assert_eq!(settings.conversation_rate_limit_per_minute, 5);
    assert_eq!(settings.claude_prompt_root, "./claude-prompt");
    assert_eq!(settings.session_db_path, "/srv/agent-state/runner.db");
    assert_eq!(settings.history_db_path, "/srv/agent-state/history.db");
}

#[test]
fn settings_parse_prompt_and_history_fields() {
    let env = std::collections::HashMap::from([
        (
            "DOGBOT_CLAUDE_PROMPT_ROOT".to_string(),
            "./custom-claude-prompt".to_string(),
        ),
        (
            "HISTORY_DB_PATH".to_string(),
            "/tmp/custom/history.db".to_string(),
        ),
    ]);

    let settings = Settings::from_env_map(env).unwrap();
    assert_eq!(settings.claude_prompt_root, "./custom-claude-prompt");
    assert_eq!(settings.history_db_path, "/tmp/custom/history.db");
}

#[test]
fn settings_parse_bifrost_fields() {
    let env = std::collections::HashMap::from([
        (
            "BIFROST_PORT".to_string(),
            "18080".to_string(),
        ),
        (
            "BIFROST_PROVIDER_NAME".to_string(),
            "gateway".to_string(),
        ),
        (
            "BIFROST_MODEL".to_string(),
            "gateway/gpt-5".to_string(),
        ),
        (
            "BIFROST_UPSTREAM_BASE_URL".to_string(),
            "https://llm-gateway.example".to_string(),
        ),
        (
            "BIFROST_UPSTREAM_API_KEY".to_string(),
            "provider-token".to_string(),
        ),
        (
            "BIFROST_UPSTREAM_PROVIDER_TYPE".to_string(),
            "anthropic".to_string(),
        ),
        (
            "ANTHROPIC_API_KEY".to_string(),
            "dummy-2".to_string(),
        ),
    ]);

    let settings = Settings::from_env_map(env).unwrap();
    assert_eq!(settings.bifrost_port, 18080);
    assert_eq!(settings.bifrost_provider_name, "gateway");
    assert_eq!(settings.bifrost_model, "gateway/gpt-5");
    assert_eq!(
        settings.bifrost_upstream_base_url,
        "https://llm-gateway.example"
    );
    assert_eq!(settings.bifrost_upstream_api_key, "provider-token");
    assert_eq!(settings.bifrost_upstream_provider_type, "anthropic");
    assert_eq!(settings.anthropic_api_key, "dummy-2");
    assert_eq!(
        settings.anthropic_base_url,
        "http://127.0.0.1:18080/anthropic"
    );
}

#[test]
fn settings_ignore_legacy_content_root_override() {
    let env = std::collections::HashMap::from([(
        "DOGBOT_CONTENT_ROOT".to_string(),
        "./legacy-content".to_string(),
    )]);

    let settings = Settings::from_env_map(env).unwrap();
    assert_eq!(settings.claude_prompt_root, "./claude-prompt");
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
            "ANTHROPIC_API_KEY".to_string(),
            "dummy-2".to_string(),
        ),
        ("BIFROST_MODEL".to_string(), "primary/gpt-5".to_string()),
        (
            "BIFROST_UPSTREAM_BASE_URL".to_string(),
            "https://models.example".to_string(),
        ),
        (
            "BIFROST_UPSTREAM_API_KEY".to_string(),
            "provider-token-2".to_string(),
        ),
        (
            "BIFROST_UPSTREAM_PROVIDER_TYPE".to_string(),
            "openai".to_string(),
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
    assert_eq!(settings.anthropic_base_url, "http://127.0.0.1:8080/anthropic");
    assert_eq!(settings.anthropic_api_key, "dummy-2");
    assert_eq!(settings.bifrost_model, "primary/gpt-5");
    assert_eq!(settings.bifrost_upstream_base_url, "https://models.example");
    assert_eq!(settings.bifrost_upstream_api_key, "provider-token-2");
    assert_eq!(settings.bifrost_upstream_provider_type, "openai");
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
