use std::collections::HashMap;
use std::env;

use thiserror::Error;

use crate::env_helpers::{optional_trimmed, parse_or_default, string_or_default};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub bind_addr: String,
    pub default_timeout_secs: u64,
    pub max_timeout_secs: u64,
    pub container_name: String,
    pub image_name: String,
    pub workspace_dir: String,
    pub state_dir: String,
    pub anthropic_base_url: String,
    pub api_proxy_auth_token: String,
    pub napcat_api_base_url: String,
    pub napcat_access_token: Option<String>,
    pub max_concurrent_runs: usize,
    pub max_queue_depth: usize,
    pub global_rate_limit_per_minute: usize,
    pub user_rate_limit_per_minute: usize,
    pub conversation_rate_limit_per_minute: usize,
    pub session_db_path: String,
    pub container_cpu_cores: u64,
    pub container_memory_mb: u64,
    pub container_disk_gb: u64,
    pub container_pids_limit: i64,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid integer for {0}")]
    InvalidInt(&'static str),
    #[error("DEFAULT_TIMEOUT_SECS must be <= MAX_TIMEOUT_SECS")]
    InvalidTimeoutBounds,
}

impl Settings {
    pub fn from_env() -> Result<Self, ConfigError> {
        let env_map = env::vars().collect::<HashMap<_, _>>();
        Self::from_env_map(env_map)
    }

    pub fn from_env_map(env_map: HashMap<String, String>) -> Result<Self, ConfigError> {
        let bind_addr = string_or_default(&env_map, "BIND_ADDR", "127.0.0.1:8787");
        let default_timeout_secs = parse_or_default(&env_map, "DEFAULT_TIMEOUT_SECS", 120)?;
        let max_timeout_secs = parse_or_default(&env_map, "MAX_TIMEOUT_SECS", 300)?;
        let container_name =
            string_or_default(&env_map, "CLAUDE_CONTAINER_NAME", "claude-runner");
        let image_name =
            string_or_default(&env_map, "CLAUDE_IMAGE_NAME", "dogbot/claude-runner:local");
        let workspace_dir = string_or_default(&env_map, "AGENT_WORKSPACE_DIR", "/srv/agent-workdir");
        let state_dir = string_or_default(&env_map, "AGENT_STATE_DIR", "/srv/agent-state");
        let anthropic_base_url =
            string_or_default(&env_map, "ANTHROPIC_BASE_URL", "http://host.docker.internal:9000");
        let api_proxy_auth_token = optional_trimmed(&env_map, "API_PROXY_AUTH_TOKEN")
            .unwrap_or_else(|| "local-proxy-token".to_string());
        let max_concurrent_runs = parse_or_default(&env_map, "MAX_CONCURRENT_RUNS", 10)?;
        let napcat_api_base_url =
            string_or_default(&env_map, "NAPCAT_API_BASE_URL", "http://127.0.0.1:3001");
        let napcat_access_token = optional_trimmed(&env_map, "NAPCAT_ACCESS_TOKEN");
        let max_queue_depth = parse_or_default(&env_map, "MAX_QUEUE_DEPTH", 20)?;
        let global_rate_limit_per_minute =
            parse_or_default(&env_map, "GLOBAL_RATE_LIMIT_PER_MINUTE", 10)?;
        let user_rate_limit_per_minute =
            parse_or_default(&env_map, "USER_RATE_LIMIT_PER_MINUTE", 3)?;
        let conversation_rate_limit_per_minute =
            parse_or_default(&env_map, "CONVERSATION_RATE_LIMIT_PER_MINUTE", 5)?;
        let session_db_path = optional_trimmed(&env_map, "SESSION_DB_PATH")
            .unwrap_or_else(|| format!("{state_dir}/runner.db"));
        let container_cpu_cores =
            parse_or_default(&env_map, "CLAUDE_CONTAINER_CPU_CORES", 4)?;
        let container_memory_mb =
            parse_or_default(&env_map, "CLAUDE_CONTAINER_MEMORY_MB", 4096)?;
        let container_disk_gb = parse_or_default(&env_map, "CLAUDE_CONTAINER_DISK_GB", 50)?;
        let container_pids_limit =
            parse_or_default(&env_map, "CLAUDE_CONTAINER_PIDS_LIMIT", 256)?;

        if default_timeout_secs > max_timeout_secs {
            return Err(ConfigError::InvalidTimeoutBounds);
        }

        Ok(Self {
            bind_addr,
            default_timeout_secs,
            max_timeout_secs,
            container_name,
            image_name,
            workspace_dir,
            state_dir,
            anthropic_base_url,
            api_proxy_auth_token,
            napcat_api_base_url,
            napcat_access_token,
            max_concurrent_runs,
            max_queue_depth,
            global_rate_limit_per_minute,
            user_rate_limit_per_minute,
            conversation_rate_limit_per_minute,
            session_db_path,
            container_cpu_cores,
            container_memory_mb,
            container_disk_gb,
            container_pids_limit,
        })
    }
}
