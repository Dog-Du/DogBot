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
    pub claude_prompt_root: String,
    pub anthropic_base_url: String,
    pub anthropic_api_key: String,
    pub bifrost_port: u16,
    pub bifrost_provider_name: String,
    pub bifrost_model: String,
    pub bifrost_upstream_base_url: String,
    pub bifrost_upstream_api_key: String,
    pub bifrost_upstream_provider_type: String,
    pub napcat_api_base_url: String,
    pub napcat_access_token: Option<String>,
    pub platform_qq_account_id: Option<String>,
    pub platform_qq_bot_id: Option<String>,
    pub platform_wechatpadpro_account_id: Option<String>,
    pub platform_wechatpadpro_bot_mention_names: Vec<String>,
    pub max_concurrent_runs: usize,
    pub max_queue_depth: usize,
    pub global_rate_limit_per_minute: usize,
    pub user_rate_limit_per_minute: usize,
    pub conversation_rate_limit_per_minute: usize,
    pub session_db_path: String,
    pub history_db_path: String,
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
        let container_name = string_or_default(&env_map, "CLAUDE_CONTAINER_NAME", "claude-runner");
        let image_name =
            string_or_default(&env_map, "CLAUDE_IMAGE_NAME", "dogbot/claude-runner:local");
        let workspace_dir =
            string_or_default(&env_map, "AGENT_WORKSPACE_DIR", "/srv/agent-workdir");
        let state_dir = string_or_default(&env_map, "AGENT_STATE_DIR", "/srv/agent-state");
        let claude_prompt_root =
            string_or_default(&env_map, "DOGBOT_CLAUDE_PROMPT_ROOT", "./claude-prompt");
        let bifrost_port = parse_or_default(&env_map, "BIFROST_PORT", 8080)?;
        let bifrost_provider_name = string_or_default(&env_map, "BIFROST_PROVIDER_NAME", "primary");
        let bifrost_model = string_or_default(&env_map, "BIFROST_MODEL", "primary/model-id");
        let bifrost_upstream_base_url = optional_trimmed(&env_map, "BIFROST_UPSTREAM_BASE_URL")
            .unwrap_or_else(|| "https://example.com".to_string());
        let bifrost_upstream_api_key = optional_trimmed(&env_map, "BIFROST_UPSTREAM_API_KEY")
            .unwrap_or_else(|| "replace-me".to_string());
        let bifrost_upstream_provider_type =
            string_or_default(&env_map, "BIFROST_UPSTREAM_PROVIDER_TYPE", "openai");
        let anthropic_base_url = optional_trimmed(&env_map, "ANTHROPIC_BASE_URL")
            .unwrap_or_else(|| format!("http://127.0.0.1:{bifrost_port}/anthropic"));
        let anthropic_api_key =
            optional_trimmed(&env_map, "ANTHROPIC_API_KEY").unwrap_or_else(|| "dummy".to_string());
        let max_concurrent_runs = parse_or_default(&env_map, "MAX_CONCURRENT_RUNS", 10)?;
        let napcat_api_base_url =
            string_or_default(&env_map, "NAPCAT_API_BASE_URL", "http://127.0.0.1:3001");
        let napcat_access_token = optional_trimmed(&env_map, "NAPCAT_ACCESS_TOKEN");
        let platform_qq_account_id = optional_trimmed(&env_map, "PLATFORM_QQ_ACCOUNT_ID");
        let platform_qq_bot_id = optional_trimmed(&env_map, "PLATFORM_QQ_BOT_ID");
        let platform_wechatpadpro_account_id =
            optional_trimmed(&env_map, "PLATFORM_WECHATPADPRO_ACCOUNT_ID");
        let platform_wechatpadpro_bot_mention_names =
            optional_trimmed(&env_map, "PLATFORM_WECHATPADPRO_BOT_MENTION_NAMES")
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
        let max_queue_depth = parse_or_default(&env_map, "MAX_QUEUE_DEPTH", 20)?;
        let global_rate_limit_per_minute =
            parse_or_default(&env_map, "GLOBAL_RATE_LIMIT_PER_MINUTE", 10)?;
        let user_rate_limit_per_minute =
            parse_or_default(&env_map, "USER_RATE_LIMIT_PER_MINUTE", 3)?;
        let conversation_rate_limit_per_minute =
            parse_or_default(&env_map, "CONVERSATION_RATE_LIMIT_PER_MINUTE", 5)?;
        let session_db_path = optional_trimmed(&env_map, "SESSION_DB_PATH")
            .unwrap_or_else(|| format!("{state_dir}/runner.db"));
        let history_db_path = optional_trimmed(&env_map, "HISTORY_DB_PATH")
            .unwrap_or_else(|| format!("{state_dir}/history.db"));
        let container_cpu_cores = parse_or_default(&env_map, "CLAUDE_CONTAINER_CPU_CORES", 4)?;
        let container_memory_mb = parse_or_default(&env_map, "CLAUDE_CONTAINER_MEMORY_MB", 4096)?;
        let container_disk_gb = parse_or_default(&env_map, "CLAUDE_CONTAINER_DISK_GB", 50)?;
        let container_pids_limit = parse_or_default(&env_map, "CLAUDE_CONTAINER_PIDS_LIMIT", 256)?;

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
            claude_prompt_root,
            anthropic_base_url,
            anthropic_api_key,
            bifrost_port,
            bifrost_provider_name,
            bifrost_model,
            bifrost_upstream_base_url,
            bifrost_upstream_api_key,
            bifrost_upstream_provider_type,
            napcat_api_base_url,
            napcat_access_token,
            platform_qq_account_id,
            platform_qq_bot_id,
            platform_wechatpadpro_account_id,
            platform_wechatpadpro_bot_mention_names,
            max_concurrent_runs,
            max_queue_depth,
            global_rate_limit_per_minute,
            user_rate_limit_per_minute,
            conversation_rate_limit_per_minute,
            session_db_path,
            history_db_path,
            container_cpu_cores,
            container_memory_mb,
            container_disk_gb,
            container_pids_limit,
        })
    }
}
