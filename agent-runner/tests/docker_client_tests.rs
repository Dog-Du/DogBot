use agent_runner::config::Settings;

#[test]
fn claude_exec_options_include_per_exec_env() {
    let options = agent_runner::docker_client::claude_exec_options(
        "/workspace",
        vec!["claude".to_string(), "hello".to_string()],
        vec![
            "DOGBOT_HISTORY_RUN_TOKEN=token-1".to_string(),
            "PGOPTIONS=-c dogbot.run_token=token-1".to_string(),
        ],
    );

    assert_eq!(
        options.env,
        Some(vec![
            "DOGBOT_HISTORY_RUN_TOKEN=token-1".to_string(),
            "PGOPTIONS=-c dogbot.run_token=token-1".to_string(),
        ])
    );
    assert_eq!(options.working_dir.as_deref(), Some("/workspace"));
    assert_eq!(
        options.cmd,
        Some(vec!["claude".to_string(), "hello".to_string()])
    );
}

#[test]
fn container_spec_matches_runner_defaults() {
    let settings = Settings::from_env_map(std::collections::HashMap::new()).unwrap();
    let spec = agent_runner::docker_client::ContainerSpec::from_settings(&settings);

    assert_eq!(spec.container_name, "claude-runner");
    assert_eq!(spec.image_name, "dogbot/claude-runner:local");
    assert_eq!(spec.workspace_dir, "/srv/agent-workdir");
    assert_eq!(spec.state_dir, "/srv/agent-state");
    assert_eq!(spec.anthropic_base_url, "http://127.0.0.1:8080/anthropic");
    assert_eq!(spec.anthropic_api_key, "dummy");
    assert_eq!(spec.bifrost_model, "primary/model-id");
    assert_eq!(spec.bifrost_upstream_base_url, "https://example.com");
    assert_eq!(spec.bifrost_upstream_api_key, "replace-me");
    assert_eq!(spec.bifrost_upstream_provider_type, "openai");
}

#[test]
fn create_container_config_carries_runtime_limits_and_mounts() {
    let settings = Settings::from_env_map(std::collections::HashMap::from([
        (
            "BIFROST_UPSTREAM_BASE_URL".to_string(),
            "https://llm-gateway.example".to_string(),
        ),
        (
            "BIFROST_UPSTREAM_API_KEY".to_string(),
            "provider-token".to_string(),
        ),
        ("BIFROST_MODEL".to_string(), "primary/gpt-5".to_string()),
        (
            "BIFROST_UPSTREAM_PROVIDER_TYPE".to_string(),
            "openai".to_string(),
        ),
    ]))
    .unwrap();
    let spec = agent_runner::docker_client::ContainerSpec::from_settings(&settings);
    let config = spec.create_config();
    let host_config = config.host_config.expect("host config");

    assert_eq!(config.image.as_deref(), Some("dogbot/claude-runner:local"));
    assert_eq!(config.working_dir.as_deref(), Some("/workspace"));
    let env = config.env.as_ref().expect("env");
    let required_env = [
        "ANTHROPIC_BASE_URL=http://127.0.0.1:8080/anthropic",
        "ANTHROPIC_API_KEY=dummy",
        "ANTHROPIC_DEFAULT_SONNET_MODEL=primary/gpt-5",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL=primary/gpt-5",
        "ANTHROPIC_DEFAULT_OPUS_MODEL=primary/gpt-5",
        "CLAUDE_CONFIG_DIR=/state/claude",
        "CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1",
        "CLAUDE_CODE_DISABLE_AUTO_MEMORY=1",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1",
        "CLAUDE_CODE_DISABLE_TERMINAL_TITLE=1",
        "CLAUDE_CODE_ATTRIBUTION_HEADER=0",
        "BIFROST_PORT=8080",
        "BIFROST_PROVIDER_NAME=primary",
        "BIFROST_MODEL=primary/gpt-5",
        "BIFROST_UPSTREAM_PROVIDER_TYPE=openai",
        "BIFROST_UPSTREAM_BASE_URL=https://llm-gateway.example",
        "BIFROST_UPSTREAM_API_KEY=provider-token",
    ];

    for required in required_env {
        assert!(
            env.iter().any(|value| value == required),
            "missing required env: {required}"
        );
    }
    assert_eq!(host_config.nano_cpus, Some(4_000_000_000));
    assert_eq!(host_config.memory, Some(4 * 1024 * 1024 * 1024));
    assert_eq!(host_config.memory_swap, Some(4 * 1024 * 1024 * 1024));
    assert_eq!(host_config.pids_limit, Some(256));
    assert!(host_config.readonly_rootfs.unwrap_or(false));
    assert_eq!(
        host_config.binds.expect("binds"),
        vec![
            "/srv/agent-workdir:/workspace".to_string(),
            "/srv/agent-state:/state".to_string(),
        ]
    );
    let tmpfs = host_config.tmpfs.expect("tmpfs");
    assert_eq!(tmpfs.get("/tmp"), Some(&"size=256m,mode=1777".to_string()));
    assert_eq!(tmpfs.get("/run"), Some(&"size=64m".to_string()));
    assert_eq!(
        host_config.extra_hosts.expect("extra_hosts"),
        vec!["host.docker.internal:host-gateway".to_string()]
    );
}
