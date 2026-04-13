use std::collections::HashMap;
use std::env;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub bind_addr: String,
    pub default_timeout_secs: u64,
    pub max_timeout_secs: u64,
    pub container_name: String,
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

        if default_timeout_secs > max_timeout_secs {
            return Err(ConfigError::InvalidTimeoutBounds);
        }

        Ok(Self {
            bind_addr,
            default_timeout_secs,
            max_timeout_secs,
            container_name,
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
