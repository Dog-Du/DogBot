use std::collections::HashMap;
use std::env;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiProxySettings {
    pub bind_addr: String,
    pub local_auth_token: String,
    pub upstream: ProviderConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    pub base_url: String,
    pub upstream_token: String,
    pub upstream_auth_header: String,
    pub upstream_auth_scheme: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required upstream proxy settings")]
    MissingUpstream,
}

impl ApiProxySettings {
    pub fn from_env() -> Result<Self, ConfigError> {
        let env_map = env::vars().collect::<HashMap<_, _>>();
        Self::from_env_map(env_map)
    }

    pub fn from_env_map(env_map: HashMap<String, String>) -> Result<Self, ConfigError> {
        Ok(Self {
            bind_addr: env_map
                .get("API_PROXY_BIND_ADDR")
                .cloned()
                .unwrap_or_else(|| "0.0.0.0:9000".to_string()),
            local_auth_token: normalize_optional_env(
                env_map.get("API_PROXY_AUTH_TOKEN").cloned(),
                "local-proxy-token",
            ),
            upstream: parse_upstream(&env_map)?,
        })
    }
}

fn parse_upstream(env_map: &HashMap<String, String>) -> Result<ProviderConfig, ConfigError> {
    let base_url = env_map
        .get("API_PROXY_UPSTREAM_BASE_URL")
        .and_then(|value| trim_to_option(value))
        .ok_or(ConfigError::MissingUpstream)?;
    let upstream_token = env_map
        .get("API_PROXY_UPSTREAM_TOKEN")
        .and_then(|value| trim_to_option(value))
        .ok_or(ConfigError::MissingUpstream)?;

    Ok(ProviderConfig {
        base_url,
        upstream_token,
        upstream_auth_header: normalize_optional_env(
            env_map.get("API_PROXY_UPSTREAM_AUTH_HEADER").cloned(),
            "x-api-key",
        ),
        upstream_auth_scheme: env_map
            .get("API_PROXY_UPSTREAM_AUTH_SCHEME")
            .and_then(|value| trim_to_option(value)),
        model: env_map
            .get("API_PROXY_UPSTREAM_MODEL")
            .and_then(|value| trim_to_option(value)),
    })
}

fn normalize_optional_env(value: Option<String>, default: &str) -> String {
    match value.and_then(|value| trim_to_option(&value)) {
        Some(value) => value,
        None => default.to_string(),
    }
}

fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
