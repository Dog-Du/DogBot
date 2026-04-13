use agent_runner::config::Settings;

#[test]
fn container_spec_matches_runner_defaults() {
    let settings = Settings::from_env_map(std::collections::HashMap::new()).unwrap();
    let spec = agent_runner::docker_client::ContainerSpec::from_settings(&settings);

    assert_eq!(spec.container_name, "claude-runner");
    assert_eq!(spec.image_name, "myqqbot/claude-runner:local");
    assert_eq!(spec.workspace_dir, "/srv/agent-workdir");
    assert_eq!(spec.state_dir, "/srv/agent-state");
    assert_eq!(spec.anthropic_base_url, "http://host.docker.internal:9000");
    assert_eq!(spec.api_proxy_auth_token, "local-proxy-token");
}

#[test]
fn create_container_config_carries_runtime_limits_and_mounts() {
    let settings = Settings::from_env_map(std::collections::HashMap::from([
        (
            "API_PROXY_AUTH_TOKEN".to_string(),
            "local-proxy-token-2".to_string(),
        ),
    ]))
    .unwrap();
    let spec = agent_runner::docker_client::ContainerSpec::from_settings(&settings);
    let config = spec.create_config();
    let host_config = config.host_config.expect("host config");

    assert_eq!(config.image.as_deref(), Some("myqqbot/claude-runner:local"));
    assert_eq!(config.working_dir.as_deref(), Some("/workspace"));
    let env = config.env.as_ref().expect("env");
    let required_env = [
        "ANTHROPIC_BASE_URL=http://host.docker.internal:9000",
        "ANTHROPIC_AUTH_TOKEN=local-proxy-token-2",
        "CLAUDE_CONFIG_DIR=/state/claude",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1",
        "CLAUDE_CODE_DISABLE_TERMINAL_TITLE=1",
        "CLAUDE_CODE_ATTRIBUTION_HEADER=0",
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
