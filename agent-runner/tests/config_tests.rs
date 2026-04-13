use agent_runner::config::Settings;

#[test]
fn settings_use_expected_defaults() {
    let settings = Settings::from_env_map(std::collections::HashMap::new()).unwrap();

    assert_eq!(settings.bind_addr, "127.0.0.1:8787");
    assert_eq!(settings.default_timeout_secs, 120);
    assert_eq!(settings.max_timeout_secs, 300);
    assert_eq!(settings.container_name, "claude-runner");
}

#[test]
fn settings_reject_default_timeout_above_max() {
    let env = std::collections::HashMap::from([
        ("DEFAULT_TIMEOUT_SECS".to_string(), "400".to_string()),
        ("MAX_TIMEOUT_SECS".to_string(), "300".to_string()),
    ]);

    let err = Settings::from_env_map(env).unwrap_err();
    assert!(err
        .to_string()
        .contains("DEFAULT_TIMEOUT_SECS must be <= MAX_TIMEOUT_SECS"));
}
