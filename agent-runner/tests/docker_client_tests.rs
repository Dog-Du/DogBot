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
}

#[test]
fn create_container_config_carries_runtime_limits_and_mounts() {
    let settings = Settings::from_env_map(std::collections::HashMap::new()).unwrap();
    let spec = agent_runner::docker_client::ContainerSpec::from_settings(&settings);
    let config = spec.create_config();
    let host_config = config.host_config.expect("host config");

    assert_eq!(config.image.as_deref(), Some("myqqbot/claude-runner:local"));
    assert_eq!(config.working_dir.as_deref(), Some("/workspace"));
    assert_eq!(
        config.env.as_ref().expect("env"),
        &vec![
            "ANTHROPIC_BASE_URL=http://host.docker.internal:9000".to_string(),
            "CLAUDE_CONFIG_DIR=/state/claude".to_string(),
        ]
    );
    assert_eq!(host_config.nano_cpus, Some(4_000_000_000));
    assert_eq!(host_config.memory, Some(4 * 1024 * 1024 * 1024));
    assert_eq!(host_config.memory_swap, Some(4 * 1024 * 1024 * 1024));
    assert_eq!(host_config.pids_limit, Some(256));
    assert_eq!(
        host_config.storage_opt.expect("storage opt").get("size"),
        Some(&"50G".to_string())
    );
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
