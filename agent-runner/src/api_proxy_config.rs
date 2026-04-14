use std::collections::HashMap;
use std::env;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiProxySettings {
    pub bind_addr: String,
    pub local_auth_token: String,
    pub active_provider: ProviderKind,
    pub packy: Option<ProviderConfig>,
    pub glm_official: Option<ProviderConfig>,
    pub minimax_official: Option<ProviderConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    pub base_url: String,
    pub upstream_token: String,
    pub upstream_auth_header: String,
    pub upstream_auth_scheme: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Packy,
    GlmOfficial,
    MinimaxOfficial,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("API_PROXY_ACTIVE_PROVIDER must be one of: packy, glm_official, minimax_official")]
    InvalidActiveProvider,
    #[error("incomplete provider configuration for {0}")]
    IncompleteProvider(&'static str),
    #[error("active provider {0} is not configured")]
    MissingActiveProvider(&'static str),
}

impl ApiProxySettings {
    pub fn from_env() -> Result<Self, ConfigError> {
        let env_map = env::vars().collect::<HashMap<_, _>>();
        Self::from_env_map(env_map)
    }

    pub fn from_env_map(env_map: HashMap<String, String>) -> Result<Self, ConfigError> {
        let active_provider = match env_map
            .get("API_PROXY_ACTIVE_PROVIDER")
            .map(|value| value.trim())
            .unwrap_or("packy")
        {
            "packy" => ProviderKind::Packy,
            "glm_official" => ProviderKind::GlmOfficial,
            "minimax_official" => ProviderKind::MinimaxOfficial,
            _ => return Err(ConfigError::InvalidActiveProvider),
        };

        let settings = Self {
            bind_addr: env_map
                .get("API_PROXY_BIND_ADDR")
                .cloned()
                .unwrap_or_else(|| "0.0.0.0:9000".to_string()),
            local_auth_token: normalize_optional_env(
                env_map.get("API_PROXY_AUTH_TOKEN").cloned(),
                "local-proxy-token",
            ),
            active_provider,
            packy: parse_provider(&env_map, "API_PROXY_PACKY", "packy")?,
            glm_official: parse_provider(&env_map, "API_PROXY_GLM", "glm_official")?,
            minimax_official: parse_provider(
                &env_map,
                "API_PROXY_MINIMAX",
                "minimax_official",
            )?,
        };

        if settings.active_provider_config().is_none() {
            return Err(ConfigError::MissingActiveProvider(
                settings.active_provider.as_str(),
            ));
        }

        Ok(settings)
    }

    pub fn active_provider_config(&self) -> Option<&ProviderConfig> {
        match self.active_provider {
            ProviderKind::Packy => self.packy.as_ref(),
            ProviderKind::GlmOfficial => self.glm_official.as_ref(),
            ProviderKind::MinimaxOfficial => self.minimax_official.as_ref(),
        }
    }
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::Packy => "packy",
            ProviderKind::GlmOfficial => "glm_official",
            ProviderKind::MinimaxOfficial => "minimax_official",
        }
    }
}

fn parse_provider(
    env_map: &HashMap<String, String>,
    prefix: &'static str,
    name: &'static str,
) -> Result<Option<ProviderConfig>, ConfigError> {
    let base_url = env_map.get(&format!("{prefix}_BASE_URL")).cloned();
    let upstream_token = env_map.get(&format!("{prefix}_UPSTREAM_TOKEN")).cloned();
    let upstream_auth_header = normalize_optional_env(
        env_map.get(&format!("{prefix}_AUTH_HEADER")).cloned(),
        "x-api-key",
    );
    let upstream_auth_scheme = env_map
        .get(&format!("{prefix}_AUTH_SCHEME"))
        .cloned()
        .and_then(|value| trim_to_option(&value));
    let model = env_map
        .get(&format!("{prefix}_MODEL"))
        .cloned()
        .and_then(|value| trim_to_option(&value));

    match (
        base_url.and_then(|value| trim_to_option(&value)),
        upstream_token.and_then(|value| trim_to_option(&value)),
    ) {
        (None, None) => Ok(None),
        (Some(base_url), Some(upstream_token)) => Ok(Some(ProviderConfig {
            base_url,
            upstream_token,
            upstream_auth_header,
            upstream_auth_scheme,
            model,
        })),
        _ => Err(ConfigError::IncompleteProvider(name)),
    }
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
