use std::collections::HashMap;
use std::env;

use thiserror::Error;

use crate::env_helpers::{optional_trimmed, string_or_default};

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
            bind_addr: string_or_default(&env_map, "API_PROXY_BIND_ADDR", "0.0.0.0:9000"),
            local_auth_token: optional_trimmed(&env_map, "API_PROXY_AUTH_TOKEN")
                .unwrap_or_else(|| "local-proxy-token".to_string()),
            upstream: parse_upstream(&env_map)?,
        })
    }
}

fn parse_upstream(env_map: &HashMap<String, String>) -> Result<ProviderConfig, ConfigError> {
    let base_url =
        optional_trimmed(env_map, "API_PROXY_UPSTREAM_BASE_URL").ok_or(ConfigError::MissingUpstream)?;
    let upstream_token =
        optional_trimmed(env_map, "API_PROXY_UPSTREAM_TOKEN").ok_or(ConfigError::MissingUpstream)?;

    Ok(ProviderConfig {
        base_url,
        upstream_token,
        upstream_auth_header: optional_trimmed(env_map, "API_PROXY_UPSTREAM_AUTH_HEADER")
            .unwrap_or_else(|| "x-api-key".to_string()),
        upstream_auth_scheme: optional_trimmed(env_map, "API_PROXY_UPSTREAM_AUTH_SCHEME"),
        model: optional_trimmed(env_map, "API_PROXY_UPSTREAM_MODEL"),
    })
}
