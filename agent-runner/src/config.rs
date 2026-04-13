use std::collections::HashMap;
use std::env;

use thiserror::Error;

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
        let bind_addr = env_map
            .get("BIND_ADDR")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1:8787".to_string());
        let default_timeout_secs = parse_u64(&env_map, "DEFAULT_TIMEOUT_SECS", 120)?;
        let max_timeout_secs = parse_u64(&env_map, "MAX_TIMEOUT_SECS", 300)?;
        let container_name = env_map
            .get("CLAUDE_CONTAINER_NAME")
            .cloned()
            .unwrap_or_else(|| "claude-runner".to_string());
        let image_name = env_map
            .get("CLAUDE_IMAGE_NAME")
            .cloned()
            .unwrap_or_else(|| "myqqbot/claude-runner:local".to_string());
        let workspace_dir = env_map
            .get("AGENT_WORKSPACE_DIR")
            .cloned()
            .unwrap_or_else(|| "/srv/agent-workdir".to_string());
        let state_dir = env_map
            .get("AGENT_STATE_DIR")
            .cloned()
            .unwrap_or_else(|| "/srv/agent-state".to_string());
        let anthropic_base_url = env_map
            .get("ANTHROPIC_BASE_URL")
            .cloned()
            .unwrap_or_else(|| "http://host.docker.internal:9000".to_string());
        let max_concurrent_runs = parse_usize(&env_map, "MAX_CONCURRENT_RUNS", 10)?;
        let napcat_api_base_url = env_map
            .get("NAPCAT_API_BASE_URL")
            .cloned()
            .unwrap_or_else(|| "http://127.0.0.1:3001".to_string());
        let napcat_access_token = env_map
            .get("NAPCAT_ACCESS_TOKEN")
            .cloned()
            .and_then(|value| {
                let trimmed = value.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            });
        let max_queue_depth = parse_usize(&env_map, "MAX_QUEUE_DEPTH", 20)?;
        let global_rate_limit_per_minute =
            parse_usize(&env_map, "GLOBAL_RATE_LIMIT_PER_MINUTE", 10)?;
        let user_rate_limit_per_minute = parse_usize(&env_map, "USER_RATE_LIMIT_PER_MINUTE", 3)?;
        let conversation_rate_limit_per_minute =
            parse_usize(&env_map, "CONVERSATION_RATE_LIMIT_PER_MINUTE", 5)?;
        let session_db_path = env_map
            .get("SESSION_DB_PATH")
            .cloned()
            .unwrap_or_else(|| format!("{state_dir}/runner.db"));
        let container_cpu_cores = parse_u64(&env_map, "CLAUDE_CONTAINER_CPU_CORES", 4)?;
        let container_memory_mb = parse_u64(&env_map, "CLAUDE_CONTAINER_MEMORY_MB", 4096)?;
        let container_disk_gb = parse_u64(&env_map, "CLAUDE_CONTAINER_DISK_GB", 50)?;
        let container_pids_limit = parse_i64(&env_map, "CLAUDE_CONTAINER_PIDS_LIMIT", 256)?;

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

fn parse_u64(
    env_map: &HashMap<String, String>,
    key: &'static str,
    default: u64,
) -> Result<u64, ConfigError> {
    match env_map.get(key) {
        Some(raw) => raw.parse().map_err(|_| ConfigError::InvalidInt(key)),
        None => Ok(default),
    }
}

fn parse_usize(
    env_map: &HashMap<String, String>,
    key: &'static str,
    default: usize,
) -> Result<usize, ConfigError> {
    match env_map.get(key) {
        Some(raw) => raw.parse().map_err(|_| ConfigError::InvalidInt(key)),
        None => Ok(default),
    }
}

fn parse_i64(
    env_map: &HashMap<String, String>,
    key: &'static str,
    default: i64,
) -> Result<i64, ConfigError> {
    match env_map.get(key) {
        Some(raw) => raw.parse().map_err(|_| ConfigError::InvalidInt(key)),
        None => Ok(default),
    }
}
